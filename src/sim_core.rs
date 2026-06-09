use crate::agent_mind::{
    ConversationContextInput, ConversationObservedAgentInput, ConversationTurnInput,
    ConversationTurnOutput, DecisionEnvelope, DecisionInput, EconomicContextInput,
    EconomicOpportunityInput, NearbyAgentInput, NearbyFixtureInput, PsychologicalContextInput,
    RecentEventInput, RelationalHistoryInput,
    retrieve_relational_memories, retrieve_relevant_memories, validate_intent,
};
use crate::llm_adapter::{LlmAdapter, LlmError};
use crate::world_model::{
    AgentIntent, AgentMemory, AgentProfile, AgentRelation, AgentSnapshot, AgentState, BuildingId,
    BuildingSpec, ConversationId, ConversationOutcome, ConversationParticipantState,
    ConversationState, ConversationStatus, ConversationTurn, EconomicNode, EconomicTask,
    EconomicTaskId, EconomicTaskKind, EconomicTaskPhase, EstablishmentEconomy, EstablishmentId,
    EventKind, FixtureId, FixtureKind, FixtureSpec, HouseholdEconomy, IntentKind,
    LocationKind, MemoryKind, PendingPaymentClaim, PostedPrice, RelationDelta, ResourceKind,
    ResourceStack, Role, RoomId, RoomSpec, ScarcityMetric, SimulationSnapshot, SocialMove,
    SpatialSnapshot, TileCoord, TileKind, TileSpec, VillageEconomy, WorldEvent, WorldGrid,
};
use anyhow::{Result, anyhow};
use bevy_ecs::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::thread;

const SNAPSHOT_SCHEMA_VERSION: u32 = 5;
const MAX_CONVERSATION_TURNS: u32 = 6;
const CONVERSATION_RECENT_TURNS_LIMIT: usize = 6;
const ROUTINE_RECONSIDERATION_MAX: u32 = 8;
const ROUTINE_HEARTBEAT_TICKS: u64 = 6;
const SOCIAL_HEARTBEAT_TICKS: u64 = 3;
const BLOCKED_RECONSIDERATION_TICKS: u32 = 2;
const DEFAULT_CARRYING_CAPACITY: i32 = 4;

#[derive(Component, Clone)]
pub struct AgentCore {
    pub id: u64,
    pub name: String,
    pub role: Role,
    pub home_building_id: Option<BuildingId>,
    pub work_building_id: Option<BuildingId>,
    pub home_bed: Option<TileCoord>,
}

#[derive(Component, Clone)]
pub struct ProfileComponent(pub AgentProfile);

#[derive(Component, Clone)]
pub struct StateComponent(pub AgentState);

#[derive(Component, Clone, Default)]
pub struct RelationComponent(pub HashMap<u64, AgentRelation>);

#[derive(Component, Clone, Default)]
pub struct MemoryComponent(pub Vec<AgentMemory>);

#[derive(Component, Clone, Default)]
pub struct InventoryComponent(pub Vec<ResourceStack>);

#[derive(Component, Clone)]
pub struct PositionComponent(pub TileCoord);

#[derive(Component, Clone, Default)]
pub struct DestinationComponent(pub Option<TileCoord>);

#[derive(Component, Clone, Default)]
pub struct DestinationLabelComponent(pub Option<String>);

#[derive(Component, Clone, Default)]
pub struct PathComponent(pub Vec<TileCoord>);

#[derive(Component, Clone, Default)]
pub struct IntentComponent(pub Option<AgentIntent>);

#[derive(Component, Clone, Default)]
pub struct ThoughtComponent(pub String);

#[derive(Component, Clone, Default)]
pub struct DecisionBudgetComponent {
    pub cooldown_until: u64,
    pub llm_calls: u64,
}

#[derive(Component, Clone, Default)]
pub struct CognitionComponent {
    pub next_reconsideration_tick: u64,
    pub blocked_ticks: u32,
    pub last_cognition_trigger: Option<String>,
    pub last_social_opportunity_signature: Option<String>,
    pub last_deliberation_hunger: i32,
    pub last_deliberation_energy: i32,
    pub last_deliberation_health: i32,
    pub last_deliberation_stress: i32,
}

#[derive(Component, Clone, Default)]
pub struct ConversationComponent {
    pub active_conversation_id: Option<ConversationId>,
    pub conversation_partner_id: Option<u64>,
    pub last_social_act: Option<String>,
    pub social_cooldown_until: u64,
}

#[derive(Component, Clone, Default)]
pub struct EconomicActivityComponent {
    pub active_task_id: Option<EconomicTaskId>,
    pub carrying: Vec<ResourceStack>,
    pub carrying_capacity: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationConfig {
    pub village_name: String,
    pub ticks_per_day: u32,
    pub max_agents: usize,
    pub relevant_memory_limit: usize,
    pub recent_event_limit: usize,
    pub grid_width: i32,
    pub grid_height: i32,
    pub world_seed: u64,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            village_name: "Santa Bruma".to_string(),
            ticks_per_day: 24,
            max_agents: 12,
            relevant_memory_limit: 5,
            recent_event_limit: 6,
            grid_width: 48,
            grid_height: 28,
            world_seed: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentView {
    pub id: u64,
    pub name: String,
    pub role: Role,
    pub household_id: Option<BuildingId>,
    pub household_name: Option<String>,
    pub area: String,
    pub building: Option<String>,
    pub room: Option<String>,
    pub position: TileCoord,
    pub destination: Option<TileCoord>,
    pub destination_label: Option<String>,
    pub path_len: usize,
    pub state: AgentState,
    pub last_intent: Option<AgentIntent>,
    pub last_thought: String,
    pub recent_memories: Vec<AgentMemory>,
    pub relations: Vec<(u64, AgentRelation)>,
    pub active_conversation_id: Option<ConversationId>,
    pub conversation_partner_name: Option<String>,
    pub conversation_turn_count: Option<u32>,
    pub conversation_summary: Option<String>,
    pub speaking_now: bool,
    pub last_social_act: Option<String>,
    pub household_treasury: i32,
    pub household_tax_arrears: i32,
    pub household_pantry: Vec<ResourceStack>,
    pub pending_salary: i32,
    pub active_task_summary: Option<String>,
    pub carrying: Vec<ResourceStack>,
    pub work_establishment_name: Option<String>,
    pub work_establishment_cash: Option<i32>,
    pub work_establishment_stock: Vec<ResourceStack>,
    pub local_prices: Vec<PostedPrice>,
    pub public_treasury: i32,
}

#[derive(Debug, Clone)]
pub struct MapRender {
    pub rows: Vec<String>,
}

pub struct Simulation {
    world: World,
    spatial: SpatialSnapshot,
    village_name: String,
    day: u32,
    tick_of_day: u32,
    total_ticks: u64,
    ticks_per_day: u32,
    next_memory_id: u64,
    relevant_memory_limit: usize,
    recent_event_limit: usize,
    events: Vec<WorldEvent>,
    next_conversation_id: ConversationId,
    conversations: Vec<ConversationState>,
    next_economic_task_id: EconomicTaskId,
    households: Vec<HouseholdEconomy>,
    establishments: Vec<EstablishmentEconomy>,
    village_economy: VillageEconomy,
    economic_tasks: Vec<EconomicTask>,
}

impl Simulation {
    pub fn seeded(config: SimulationConfig) -> Self {
        let mut world = World::new();
        let spatial = generate_village(config.grid_width, config.grid_height, config.world_seed);
        let work_map = work_building_map(&spatial);
        let home_beds = home_bed_assignments(&spatial);
        let agent_templates = seeded_agents();

        for (template, (home_building_id, home_bed)) in agent_templates
            .into_iter()
            .take(config.max_agents)
            .zip(home_beds.into_iter())
        {
            let initial_state = template.state.clone();
            world.spawn((
                AgentCore {
                    id: template.id,
                    name: template.name,
                    role: template.role,
                    home_building_id: Some(home_building_id),
                    work_building_id: work_map.get(&template.role).copied(),
                    home_bed: Some(home_bed),
                },
                ProfileComponent(template.profile),
                StateComponent(template.state),
                RelationComponent(template.relations),
                MemoryComponent(template.memories),
                InventoryComponent(template.inventory),
                PositionComponent(home_bed),
                DestinationComponent::default(),
                DestinationLabelComponent::default(),
                PathComponent::default(),
                IntentComponent::default(),
                ThoughtComponent(template.last_thought),
                DecisionBudgetComponent::default(),
                CognitionComponent {
                    next_reconsideration_tick: 0,
                    blocked_ticks: 0,
                    last_cognition_trigger: None,
                    last_social_opportunity_signature: None,
                    last_deliberation_hunger: initial_state.hunger,
                    last_deliberation_energy: initial_state.energy,
                    last_deliberation_health: initial_state.health,
                    last_deliberation_stress: initial_state.stress,
                },
                (
                    ConversationComponent::default(),
                    EconomicActivityComponent {
                        active_task_id: None,
                        carrying: Vec::new(),
                        carrying_capacity: DEFAULT_CARRYING_CAPACITY,
                    },
                ),
            ));
        }

        let (households, establishments, village_economy) =
            initialize_economy_state(&mut world, &spatial);

        Self {
            world,
            spatial,
            village_name: config.village_name,
            day: 1,
            tick_of_day: 0,
            total_ticks: 0,
            ticks_per_day: config.ticks_per_day,
            next_memory_id: 10_000,
            relevant_memory_limit: config.relevant_memory_limit,
            recent_event_limit: config.recent_event_limit,
            events: Vec::new(),
            next_conversation_id: 1,
            conversations: Vec::new(),
            next_economic_task_id: 1,
            households,
            establishments,
            village_economy,
            economic_tasks: Vec::new(),
        }
    }

    pub fn from_snapshot(snapshot: SimulationSnapshot) -> Self {
        let mut world = World::new();
        let conversations = snapshot.conversations.clone();
        let next_conversation_id = snapshot.next_conversation_id;
        for agent in snapshot.agents {
            world.spawn((
                AgentCore {
                    id: agent.id,
                    name: agent.name,
                    role: agent.role,
                    home_building_id: agent.home_building_id,
                    work_building_id: agent.work_building_id,
                    home_bed: agent.home_bed,
                },
                ProfileComponent(agent.profile),
                StateComponent(agent.state),
                RelationComponent(agent.relations),
                MemoryComponent(agent.memories),
                InventoryComponent(agent.inventory),
                PositionComponent(agent.position),
                DestinationComponent(agent.destination),
                DestinationLabelComponent(agent.destination_label),
                PathComponent(agent.planned_path),
                IntentComponent(agent.last_intent),
                ThoughtComponent(agent.last_thought),
                DecisionBudgetComponent {
                    cooldown_until: agent.llm_cooldown_until,
                    llm_calls: agent.llm_calls,
                },
                CognitionComponent {
                    next_reconsideration_tick: agent.next_reconsideration_tick,
                    blocked_ticks: agent.blocked_ticks,
                    last_cognition_trigger: agent.last_cognition_trigger,
                    last_social_opportunity_signature: agent.last_social_opportunity_signature,
                    last_deliberation_hunger: agent.last_deliberation_hunger,
                    last_deliberation_energy: agent.last_deliberation_energy,
                    last_deliberation_health: agent.last_deliberation_health,
                    last_deliberation_stress: agent.last_deliberation_stress,
                },
                (
                    ConversationComponent {
                        active_conversation_id: agent.active_conversation_id,
                        conversation_partner_id: agent.conversation_partner_id,
                        last_social_act: agent.last_social_act,
                        social_cooldown_until: agent.social_cooldown_until,
                    },
                    EconomicActivityComponent {
                        active_task_id: agent.active_economic_task_id,
                        carrying: agent.carrying,
                        carrying_capacity: agent.carrying_capacity,
                    },
                ),
            ));
        }

        Self {
            world,
            spatial: snapshot.spatial,
            village_name: snapshot.village_name,
            day: snapshot.day,
            tick_of_day: snapshot.tick_of_day,
            total_ticks: snapshot.total_ticks,
            ticks_per_day: snapshot.ticks_per_day,
            next_memory_id: snapshot.next_memory_id,
            relevant_memory_limit: 5,
            recent_event_limit: 6,
            events: snapshot.events,
            next_conversation_id,
            conversations,
            next_economic_task_id: snapshot.next_economic_task_id,
            households: snapshot.households,
            establishments: snapshot.establishments,
            village_economy: snapshot.village_economy,
            economic_tasks: snapshot.economic_tasks,
        }
    }

    pub fn tick(&mut self, llm: &dyn LlmAdapter) -> Result<()> {
        self.total_ticks += 1;
        self.tick_of_day += 1;
        let crossed_day = self.tick_of_day >= self.ticks_per_day;
        if self.tick_of_day >= self.ticks_per_day {
            self.tick_of_day = 0;
            self.day += 1;
        }

        if crossed_day {
            self.close_daily_economy()?;
        }

        self.apply_needs_decay();
        self.refresh_economy_state()?;
        let agent_ids = self.agent_ids();

        for agent_id in &agent_ids {
            self.advance_agent_movement(*agent_id)?;
        }

        for agent_id in &agent_ids {
            self.ensure_navigation_for_current_intent(*agent_id)?;
        }

        for agent_id in &agent_ids {
            self.try_execute_current_intent(*agent_id, llm)?;
        }

        self.process_active_conversations(llm)?;
        self.process_general_decisions(llm)?;

        Ok(())
    }

    pub fn snapshot(&mut self) -> SimulationSnapshot {
        let mut agents = Vec::new();
        for agent_id in self.agent_ids() {
            let entity = self
                .find_agent_entity(agent_id)
                .expect("agent entity should exist during snapshot");
            let entry = self.world.entity(entity);
            let core = entry.get::<AgentCore>().expect("missing agent core");
            let profile = entry
                .get::<ProfileComponent>()
                .expect("missing profile component");
            let state = entry.get::<StateComponent>().expect("missing state component");
            let relations = entry
                .get::<RelationComponent>()
                .expect("missing relation component");
            let memories = entry
                .get::<MemoryComponent>()
                .expect("missing memory component");
            let inventory = entry
                .get::<InventoryComponent>()
                .expect("missing inventory component");
            let position = entry
                .get::<PositionComponent>()
                .expect("missing position component");
            let destination = entry
                .get::<DestinationComponent>()
                .expect("missing destination component");
            let destination_label = entry
                .get::<DestinationLabelComponent>()
                .expect("missing destination label component");
            let path = entry.get::<PathComponent>().expect("missing path component");
            let intent = entry.get::<IntentComponent>().expect("missing intent component");
            let thought = entry.get::<ThoughtComponent>().expect("missing thought component");
            let budget = entry
                .get::<DecisionBudgetComponent>()
                .expect("missing budget component");
            let cognition = entry
                .get::<CognitionComponent>()
                .expect("missing cognition component");
            let conversation = entry
                .get::<ConversationComponent>()
                .expect("missing conversation component");
            let economic = entry
                .get::<EconomicActivityComponent>()
                .expect("missing economy component");
            agents.push(AgentSnapshot {
                id: core.id,
                name: core.name.clone(),
                role: core.role,
                home_building_id: core.home_building_id,
                work_building_id: core.work_building_id,
                home_bed: core.home_bed,
                profile: profile.0.clone(),
                state: state.0.clone(),
                relations: relations.0.clone(),
                memories: memories.0.clone(),
                inventory: inventory.0.clone(),
                position: position.0,
                destination: destination.0,
                destination_label: destination_label.0.clone(),
                planned_path: path.0.clone(),
                current_building_id: self.tile_at(position.0).and_then(|tile| tile.building_id),
                current_room_id: self.tile_at(position.0).and_then(|tile| tile.room_id),
                active_conversation_id: conversation.active_conversation_id,
                conversation_partner_id: conversation.conversation_partner_id,
                last_social_act: conversation.last_social_act.clone(),
                social_cooldown_until: conversation.social_cooldown_until,
                last_intent: intent.0.clone(),
                last_thought: thought.0.clone(),
                llm_cooldown_until: budget.cooldown_until,
                llm_calls: budget.llm_calls,
                active_economic_task_id: economic.active_task_id,
                carrying: economic.carrying.clone(),
                carrying_capacity: economic.carrying_capacity,
                next_reconsideration_tick: cognition.next_reconsideration_tick,
                blocked_ticks: cognition.blocked_ticks,
                last_cognition_trigger: cognition.last_cognition_trigger.clone(),
                last_social_opportunity_signature: cognition
                    .last_social_opportunity_signature
                    .clone(),
                last_deliberation_hunger: cognition.last_deliberation_hunger,
                last_deliberation_energy: cognition.last_deliberation_energy,
                last_deliberation_health: cognition.last_deliberation_health,
                last_deliberation_stress: cognition.last_deliberation_stress,
            });
        }

        SimulationSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            village_name: self.village_name.clone(),
            day: self.day,
            tick_of_day: self.tick_of_day,
            total_ticks: self.total_ticks,
            ticks_per_day: self.ticks_per_day,
            next_memory_id: self.next_memory_id,
            next_conversation_id: self.next_conversation_id,
            next_economic_task_id: self.next_economic_task_id,
            agents,
            conversations: self.conversations.clone(),
            households: self.households.clone(),
            establishments: self.establishments.clone(),
            village_economy: self.village_economy.clone(),
            economic_tasks: self.economic_tasks.clone(),
            spatial: self.spatial.clone(),
            events: self.events.clone(),
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "{} | Dia {} | Tick {} | Total {}",
            self.village_name, self.day, self.tick_of_day, self.total_ticks
        )
    }

    pub fn village_name(&self) -> &str {
        &self.village_name
    }

    pub fn current_day(&self) -> u32 {
        self.day
    }

    pub fn tick_of_day(&self) -> u32 {
        self.tick_of_day
    }

    pub fn total_ticks(&self) -> u64 {
        self.total_ticks
    }

    pub fn spatial(&self) -> &SpatialSnapshot {
        &self.spatial
    }

    pub fn recent_events(&self, limit: usize) -> Vec<WorldEvent> {
        self.events.iter().rev().take(limit).cloned().collect()
    }

    pub fn economy_overview(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "caixa_publico={} | imposto_diario_por_lar={}",
            self.village_economy.public_treasury, self.village_economy.daily_household_tax
        )];
        for establishment in self.establishments.iter().filter(|establishment| {
            matches!(
                establishment.kind,
                LocationKind::Farm
                    | LocationKind::Woodlot
                    | LocationKind::Quarry
                    | LocationKind::Workshop
            )
        }) {
            let stock = establishment
                .stock
                .iter()
                .filter(|stack| {
                    matches!(
                        stack.kind,
                        ResourceKind::Graos
                            | ResourceKind::Lenha
                            | ResourceKind::MetalBruto
                            | ResourceKind::Ferramentas
                    )
                })
                .map(|stack| format!("{}x{}", stack.kind.as_str(), stack.amount))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("{} | caixa={} | {}", establishment.name, establishment.cash, stock));
        }
        lines
    }

    pub fn agent_views(&mut self) -> Vec<AgentView> {
        let agent_name_map = self.agent_name_map();
        let conversation_map = self.conversation_map();
        let mut views = Vec::new();
        let mut query = self.world.query::<(
            &AgentCore,
            &StateComponent,
            &PositionComponent,
            &DestinationComponent,
            &DestinationLabelComponent,
            &PathComponent,
            &IntentComponent,
            &ThoughtComponent,
            &MemoryComponent,
            &RelationComponent,
            &ConversationComponent,
            &EconomicActivityComponent,
        )>();
        for (
            core,
            state,
            position,
            destination,
            destination_label,
            path,
            intent,
            thought,
            memories,
            relations,
            conversation,
            economic,
        ) in query.iter(&self.world)
        {
            let tile = self.tile_at(position.0);
            let building = tile
                .and_then(|entry| entry.building_id)
                .and_then(|id| self.building_name(id));
            let room = tile
                .and_then(|entry| entry.room_id)
                .and_then(|id| self.room_name(id));
            let household = core
                .home_building_id
                .and_then(|building_id| self.household_by_id(building_id));
            let pending_salary = household
                .map(|entry| entry.pending_payments.iter().map(|claim| claim.amount).sum())
                .unwrap_or(0);
            let work_establishment = core
                .work_building_id
                .and_then(|building_id| self.establishment_by_building(building_id));
            views.push(AgentView {
                id: core.id,
                name: core.name.clone(),
                role: core.role,
                household_id: core.home_building_id,
                household_name: household.map(|entry| entry.name.clone()),
                area: self.area_name(position.0),
                building,
                room,
                position: position.0,
                destination: destination.0,
                destination_label: destination_label.0.clone(),
                path_len: path.0.len(),
                state: state.0.clone(),
                last_intent: intent.0.clone(),
                last_thought: thought.0.clone(),
                recent_memories: memories.0.iter().rev().take(4).cloned().collect(),
                relations: relations
                    .0
                    .iter()
                    .map(|(id, relation)| (*id, relation.clone()))
                    .collect(),
                active_conversation_id: conversation.active_conversation_id,
                conversation_partner_name: conversation
                    .conversation_partner_id
                    .and_then(|partner_id| agent_name_map.get(&partner_id).cloned()),
                conversation_turn_count: conversation.active_conversation_id.and_then(
                    |conversation_id| {
                        conversation_map
                            .get(&conversation_id)
                            .map(|conversation| conversation.turn_count)
                    },
                ),
                conversation_summary: conversation.active_conversation_id.and_then(
                    |conversation_id| {
                        conversation_map
                            .get(&conversation_id)
                            .map(|conversation| conversation.summary.clone())
                    },
                ),
                speaking_now: conversation
                    .active_conversation_id
                    .and_then(|conversation_id| {
                        conversation_map
                            .get(&conversation_id)
                            .map(|conversation| conversation.current_speaker_id == core.id)
                    })
                    .unwrap_or(false),
                last_social_act: conversation.last_social_act.clone(),
                household_treasury: household.map(|entry| entry.treasury).unwrap_or(0),
                household_tax_arrears: household.map(|entry| entry.tax_arrears).unwrap_or(0),
                household_pantry: household
                    .map(|entry| entry.pantry.clone())
                    .unwrap_or_default(),
                pending_salary,
                active_task_summary: economic
                    .active_task_id
                    .and_then(|task_id| self.economic_task_summary(task_id)),
                carrying: economic.carrying.clone(),
                work_establishment_name: work_establishment.map(|entry| entry.name.clone()),
                work_establishment_cash: work_establishment.map(|entry| entry.cash),
                work_establishment_stock: work_establishment
                    .map(|entry| entry.stock.clone())
                    .unwrap_or_default(),
                local_prices: self.local_prices_for_agent(position.0),
                public_treasury: self.village_economy.public_treasury,
            });
        }
        views.sort_by(|a, b| a.name.cmp(&b.name));
        views
    }

    pub fn render_ascii_map(
        &mut self,
        selected_agent_id: Option<u64>,
        width: usize,
        height: usize,
    ) -> MapRender {
        let occupancy = self.occupancy_map();
        let engaged_agents = self.active_conversation_participants();
        let selected_path = selected_agent_id.and_then(|agent_id| self.agent_path(agent_id));
        let center = selected_agent_id
            .and_then(|agent_id| self.debug_agent_position(agent_id).ok())
            .unwrap_or(TileCoord {
                x: self.spatial.grid.width / 2,
                y: self.spatial.grid.height / 2,
            });

        let half_w = width as i32 / 2;
        let half_h = height as i32 / 2;
        let mut rows = Vec::new();

        for y in (center.y - half_h)..(center.y - half_h + height as i32) {
            let mut row = String::new();
            for x in (center.x - half_w)..(center.x - half_w + width as i32) {
                let coord = TileCoord { x, y };
                let mut ch = if let Some(tile) = self.tile_at(coord) {
                    tile.kind.glyph()
                } else {
                    ' '
                };

                if let Some(fixture) = self.fixture_at(coord) {
                    ch = fixture.kind.glyph();
                }
                if let Some(path) = &selected_path {
                    if path.contains(&coord) {
                        ch = '*';
                    }
                }
                if let Some(agent_id) = occupancy.get(&coord) {
                    ch = if Some(*agent_id) == selected_agent_id {
                        '@'
                    } else if engaged_agents.contains(agent_id) {
                        '&'
                    } else {
                        self.agent_initial(*agent_id).unwrap_or('a')
                    };
                }
                row.push(ch);
            }
            rows.push(row);
        }

        MapRender { rows }
    }

    pub fn debug_agent_position(&mut self, agent_id: u64) -> Result<TileCoord> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<PositionComponent>()
            .ok_or_else(|| anyhow!("missing position component"))?
            .0)
    }

    pub fn debug_force_agent_position(&mut self, agent_id: u64, coord: TileCoord) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<PositionComponent>()
            .ok_or_else(|| anyhow!("missing position component"))?
            .0 = coord;
        self.world
            .entity_mut(entity)
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
        self.world
            .entity_mut(entity)
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        Ok(())
    }

    pub fn debug_find_path(
        &mut self,
        start: TileCoord,
        goal: TileCoord,
        ignore_agent_id: Option<u64>,
    ) -> Option<Vec<TileCoord>> {
        self.find_path(start, goal, ignore_agent_id)
    }

    pub fn debug_try_social(
        &mut self,
        actor_id: u64,
        target_id: u64,
        _llm: &dyn LlmAdapter,
    ) -> Result<bool> {
        if !self.agents_adjacent(actor_id, target_id)? {
            return Ok(false);
        }
        self.open_conversation(actor_id, target_id, SocialMove::Chat, "contato direto")
    }

    pub fn debug_add_memory(
        &mut self,
        agent_id: u64,
        kind: MemoryKind,
        summary: String,
        tags: Vec<String>,
        weight: i32,
        about: Vec<u64>,
    ) -> Result<()> {
        self.add_memory(agent_id, kind, summary, tags, weight, about)
    }

    pub fn debug_set_relation(
        &mut self,
        agent_id: u64,
        other_id: u64,
        relation: AgentRelation,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<RelationComponent>()
            .ok_or_else(|| anyhow!("missing relation component"))?
            .0
            .insert(other_id, relation);
        Ok(())
    }

    fn apply_needs_decay(&mut self) {
        let mut query = self.world.query::<&mut StateComponent>();
        for mut state in query.iter_mut(&mut self.world) {
            state.0.hunger = (state.0.hunger + 5).clamp(0, 100);
            state.0.energy = (state.0.energy - 4).clamp(0, 100);
            state.0.stress = (state.0.stress + 2).clamp(0, 100);
            if state.0.hunger > 85 || state.0.energy < 15 {
                state.0.health = (state.0.health - 2).clamp(0, 100);
            }
        }
    }

    fn agent_ids(&mut self) -> Vec<u64> {
        let mut query = self.world.query::<&AgentCore>();
        query.iter(&self.world).map(|core| core.id).collect()
    }

    fn collect_contexts(&mut self) -> Vec<AgentContext> {
        let mut query = self.world.query::<(
            &AgentCore,
            &ProfileComponent,
            &StateComponent,
            &RelationComponent,
            &MemoryComponent,
            &PositionComponent,
            &DestinationComponent,
            &PathComponent,
            &DestinationLabelComponent,
            &IntentComponent,
            &DecisionBudgetComponent,
            &CognitionComponent,
            &ConversationComponent,
        )>();
        query
            .iter(&self.world)
            .map(
                |(
                    core,
                    profile,
                    state,
                    relations,
                    memories,
                    position,
                    destination,
                    path,
                    destination_label,
                    intent,
                    budget,
                    cognition,
                    conversation,
                )| {
                    let tile = self.tile_at(position.0);
                    AgentContext {
                        id: core.id,
                        name: core.name.clone(),
                        role: core.role,
                        position: position.0,
                        state: state.0.clone(),
                        profile: profile.0.clone(),
                        relations: relations.0.clone(),
                        memories: memories.0.clone(),
                        current_destination: destination.0,
                        path_len: path.0.len(),
                        destination_label: destination_label.0.clone(),
                        current_building_id: tile.and_then(|entry| entry.building_id),
                        current_room_id: tile.and_then(|entry| entry.room_id),
                        last_intent: intent.0.clone(),
                        cooldown_until: budget.cooldown_until,
                        llm_calls: budget.llm_calls,
                        next_reconsideration_tick: cognition.next_reconsideration_tick,
                        blocked_ticks: cognition.blocked_ticks,
                        last_social_opportunity_signature: cognition
                            .last_social_opportunity_signature
                            .clone(),
                        last_deliberation_hunger: cognition.last_deliberation_hunger,
                        last_deliberation_energy: cognition.last_deliberation_energy,
                        last_deliberation_health: cognition.last_deliberation_health,
                        last_deliberation_stress: cognition.last_deliberation_stress,
                        active_conversation_id: conversation.active_conversation_id,
                        social_cooldown_until: conversation.social_cooldown_until,
                        household_id: core.home_building_id,
                    }
                },
            )
            .collect()
    }

    fn assign_intent(
        &mut self,
        agent_id: u64,
        intent: AgentIntent,
        reflection: String,
    ) -> Result<()> {
        let normalized_horizon = self.normalized_reconsideration_horizon(intent.kind);
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let current_state = entity_mut
            .get::<StateComponent>()
            .ok_or_else(|| anyhow!("missing state component"))?
            .0
            .clone();
        entity_mut
            .get_mut::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0 = Some(intent.clone());
        entity_mut
            .get_mut::<ThoughtComponent>()
            .ok_or_else(|| anyhow!("missing thought component"))?
            .0 = reflection.clone();
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        entity_mut
            .get_mut::<DestinationLabelComponent>()
            .ok_or_else(|| anyhow!("missing destination label component"))?
            .0 = intent.target_semantic.clone();
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
        {
            let mut budget = entity_mut
                .get_mut::<DecisionBudgetComponent>()
                .ok_or_else(|| anyhow!("missing budget component"))?;
            budget.cooldown_until = self.total_ticks + normalized_horizon;
            budget.llm_calls += 1;
        }
        {
            let mut cognition = entity_mut
                .get_mut::<CognitionComponent>()
                .ok_or_else(|| anyhow!("missing cognition component"))?;
            cognition.next_reconsideration_tick = self.total_ticks + normalized_horizon;
            cognition.blocked_ticks = 0;
            cognition.last_cognition_trigger = Some("novo_plano".to_string());
            cognition.last_deliberation_hunger = current_state.hunger;
            cognition.last_deliberation_energy = current_state.energy;
            cognition.last_deliberation_health = current_state.health;
            cognition.last_deliberation_stress = current_state.stress;
        }
        {
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.current_focus = intent.kind.as_str().to_string();
            for belief in &intent.belief_updates {
                if !state.0.active_goals.iter().any(|goal| goal == belief) {
                    state.0.active_goals.push(belief.clone());
                }
            }
            if state.0.active_goals.len() > 4 {
                state.0.active_goals.truncate(4);
            }
        }
        drop(entity_mut);
        if matches!(
            intent.kind,
            IntentKind::Comprar
                | IntentKind::Transportar
                | IntentKind::Vender
                | IntentKind::ReceberPagamento
                | IntentKind::Trabalhar
        ) {
            self.bind_or_create_economic_task(agent_id, &intent)?;
        } else {
            self.clear_active_economic_task(agent_id)?;
        }
        self.add_memory(
            agent_id,
            MemoryKind::Reflection,
            format!("Reflexao: {}", reflection),
            intent.belief_updates.clone(),
            12,
            intent.target_agent.into_iter().collect(),
        )?;
        Ok(())
    }

    fn bind_or_create_economic_task(&mut self, agent_id: u64, intent: &AgentIntent) -> Result<()> {
        let Some(household_id) = self.household_id_for_agent(agent_id) else {
            return Ok(());
        };
        let agent_entity = self.find_agent_entity(agent_id)?;
        let agent_role = self
            .world
            .entity(agent_entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .role;
        self.clear_active_economic_task(agent_id)?;
        let desired_kind = match intent.kind {
            IntentKind::Trabalhar => Some(EconomicTaskKind::Produzir),
            IntentKind::Comprar => Some(EconomicTaskKind::Comprar),
            IntentKind::Transportar => Some(EconomicTaskKind::Transportar),
            IntentKind::Vender => Some(EconomicTaskKind::Vender),
            IntentKind::ReceberPagamento => Some(EconomicTaskKind::ReceberPagamento),
            _ => None,
        };
        let Some(desired_kind) = desired_kind else {
            return Ok(());
        };
        let target_hint = intent
            .target_semantic
            .clone()
            .unwrap_or_default()
            .to_lowercase();
        let matches_target = |task: &EconomicTask| {
            if target_hint.is_empty() {
                return true;
            }
            let description = task.description.to_lowercase();
            let resource_match = task
                .resource
                .map(|resource| target_hint.contains(resource.as_str()))
                .unwrap_or(false);
            let household_food_match = matches!(task.destination, EconomicNode::HouseholdPantry(_))
                && (target_hint.contains("comida")
                    || target_hint.contains("lar")
                    || target_hint.contains("despensa"));
            let production_match = desired_kind == EconomicTaskKind::Produzir
                && (description.contains("lenha") && target_hint.contains("lenha")
                    || description.contains("metal") && target_hint.contains("metal")
                    || description.contains("graos") && target_hint.contains("graos")
                    || target_hint.contains("trabalho"));
            description.contains(&target_hint)
                || resource_match
                || household_food_match
                || production_match
        };
        let role_allows_task = |task: &EconomicTask| match desired_kind {
            EconomicTaskKind::Produzir => match agent_role {
                Role::Farmer => matches!(
                    task.resource,
                    Some(ResourceKind::Graos | ResourceKind::Lenha | ResourceKind::MetalBruto)
                ),
                Role::Blacksmith => task.resource == Some(ResourceKind::Ferramentas),
                Role::Baker => task.resource == Some(ResourceKind::Pao),
                Role::TavernKeeper => task.resource == Some(ResourceKind::Caldo),
                Role::Guard | Role::Headman => false,
            },
            EconomicTaskKind::Comprar
            | EconomicTaskKind::Transportar
            | EconomicTaskKind::Vender
            | EconomicTaskKind::ReceberPagamento => true,
        };

        let mut selected_task_id = self
            .economic_tasks
            .iter()
            .find(|task| {
                task.actor_household_id == household_id
                    && task.kind == desired_kind
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
                    && (task.assigned_agent_id.is_none() || task.assigned_agent_id == Some(agent_id))
                    && matches_target(task)
                    && role_allows_task(task)
            })
            .map(|task| task.id);

        if selected_task_id.is_none() {
            self.ensure_economic_tasks();
            selected_task_id = self
                .economic_tasks
                .iter()
                .find(|task| {
                    task.actor_household_id == household_id
                        && task.kind == desired_kind
                        && task.phase != EconomicTaskPhase::Completed
                        && task.phase != EconomicTaskPhase::Failed
                        && task.assigned_agent_id.is_none()
                        && matches_target(task)
                        && role_allows_task(task)
                })
                .map(|task| task.id);
        }

        if let Some(task_id) = selected_task_id {
            if let Some(task) = self.economic_tasks.iter_mut().find(|task| task.id == task_id) {
                task.assigned_agent_id = Some(agent_id);
            }
            let entity = self.find_agent_entity(agent_id)?;
            self.world
                .entity_mut(entity)
                .get_mut::<EconomicActivityComponent>()
                .ok_or_else(|| anyhow!("missing economy component"))?
                .active_task_id = Some(task_id);
        }
        Ok(())
    }

    fn clear_active_economic_task(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let previous_task_id = self
            .world
            .entity(entity)
            .get::<EconomicActivityComponent>()
            .ok_or_else(|| anyhow!("missing economy component"))?
            .active_task_id;
        if let Some(task_id) = previous_task_id
            && let Some(task) = self.economic_tasks.iter_mut().find(|task| task.id == task_id)
            && task.phase != EconomicTaskPhase::Completed
            && task.phase != EconomicTaskPhase::Failed
        {
            task.assigned_agent_id = None;
        }
        self.world
            .entity_mut(entity)
            .get_mut::<EconomicActivityComponent>()
            .ok_or_else(|| anyhow!("missing economy component"))?
            .active_task_id = None;
        Ok(())
    }

    fn ensure_navigation_for_current_intent(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let (
            intent,
            current_pos,
            current_destination,
            current_path_len,
            active_conversation_id,
            core,
        ) = {
            let entry = self.world.entity(entity);
            (
                entry
                    .get::<IntentComponent>()
                    .ok_or_else(|| anyhow!("missing intent component"))?
                    .0
                    .clone(),
                entry
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0,
                entry
                    .get::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0,
                entry
                    .get::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0
                    .len(),
                entry
                    .get::<ConversationComponent>()
                    .ok_or_else(|| anyhow!("missing conversation component"))?
                    .active_conversation_id,
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .clone(),
            )
        };
        if active_conversation_id.is_some() {
            return Ok(());
        }
        let Some(intent) = intent else {
            return Ok(());
        };
        if current_path_len > 0 {
            return Ok(());
        }

        if self.ready_to_execute(agent_id, &intent)? {
            return Ok(());
        }

        let candidates = self.resolve_intent_candidates(&core, current_pos, &intent)?;
        if candidates.is_empty()
            && matches!(
                intent.kind,
                IntentKind::Comprar
                    | IntentKind::Transportar
                    | IntentKind::Vender
                    | IntentKind::ReceberPagamento
            )
        {
            self.clear_intent_navigation(agent_id)?;
            self.clear_active_economic_task(agent_id)?;
            return Ok(());
        }
        for candidate in candidates {
            if current_pos == candidate.destination {
                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0 = Some(candidate.destination);
                entity_mut
                    .get_mut::<DestinationLabelComponent>()
                    .ok_or_else(|| anyhow!("missing destination label component"))?
                    .0 = Some(candidate.label);
                return Ok(());
            }
            if let Some(path) = self.find_path(current_pos, candidate.destination, Some(agent_id)) {
                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0 = Some(candidate.destination);
                entity_mut
                    .get_mut::<DestinationLabelComponent>()
                    .ok_or_else(|| anyhow!("missing destination label component"))?
                    .0 = Some(candidate.label);
                entity_mut
                    .get_mut::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0 = path;
                return Ok(());
            }
        }

        if current_destination.is_some() {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Blocking,
                summary: format!(
                    "{} nao encontra caminho livre para {}.",
                    core.name,
                    intent
                        .target_semantic
                        .clone()
                        .unwrap_or_else(|| intent.kind.as_str().to_string())
                ),
                impact_tags: vec!["bloqueio".to_string(), "navegacao".to_string()],
            });
        }
        Ok(())
    }

    fn try_execute_current_intent(&mut self, agent_id: u64, llm: &dyn LlmAdapter) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        if self
            .world
            .entity(entity)
            .get::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?
            .active_conversation_id
            .is_some()
        {
            return Ok(());
        }
        let intent = self
            .world
            .entity(entity)
            .get::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0
            .clone();
        let Some(intent) = intent else {
            return Ok(());
        };
        if !self.ready_to_execute(agent_id, &intent)? {
            return Ok(());
        }
        match intent.kind {
            IntentKind::Trabalhar => self.apply_work(agent_id)?,
            IntentKind::Descansar => self.apply_rest(agent_id)?,
            IntentKind::Comer => self.apply_eat(agent_id)?,
            IntentKind::Refletir => self.apply_reflect(agent_id)?,
            IntentKind::Andar => self.apply_wander(agent_id)?,
            IntentKind::Comprar
            | IntentKind::Transportar
            | IntentKind::Vender
            | IntentKind::ReceberPagamento => self.apply_economic_intent(agent_id)?,
            IntentKind::Socializar => {
                if let Some(target_id) = intent.target_agent {
                    if self.agents_adjacent(agent_id, target_id)?
                        && self.open_conversation(
                            agent_id,
                            target_id,
                            intent.social_move.unwrap_or(SocialMove::Chat),
                            &intent.justification,
                        )?
                    {
                        let _ = llm.provider_name();
                    }
                }
            }
        }
        self.reset_blocked_ticks(agent_id)?;
        match intent.kind {
            IntentKind::Socializar => self.clear_intent_navigation(agent_id)?,
            IntentKind::Andar => self.clear_navigation_keep_intent(agent_id)?,
            IntentKind::Comer => {
                if self.agent_state(agent_id)?.hunger <= 25 {
                    self.clear_intent_navigation(agent_id)?;
                }
            }
            IntentKind::Descansar => {
                if self.agent_state(agent_id)?.energy >= 80 {
                    self.clear_intent_navigation(agent_id)?;
                }
            }
            IntentKind::Refletir => {
                if self.agent_state(agent_id)?.stress <= 25 {
                    self.clear_intent_navigation(agent_id)?;
                }
            }
            IntentKind::Trabalhar => {}
            IntentKind::Comprar
            | IntentKind::Transportar
            | IntentKind::Vender
            | IntentKind::ReceberPagamento => {
                if self
                    .active_economic_task_for_agent(agent_id)
                    .map(|task| task.phase == EconomicTaskPhase::Completed)
                    .unwrap_or(true)
                {
                    self.clear_intent_navigation(agent_id)?;
                    self.clear_active_economic_task(agent_id)?;
                }
            }
        }
        Ok(())
    }

    fn clear_navigation_keep_intent(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        entity_mut
            .get_mut::<DestinationLabelComponent>()
            .ok_or_else(|| anyhow!("missing destination label component"))?
            .0 = None;
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
        Ok(())
    }

    fn clear_intent_navigation(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0 = None;
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        entity_mut
            .get_mut::<DestinationLabelComponent>()
            .ok_or_else(|| anyhow!("missing destination label component"))?
            .0 = None;
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
        Ok(())
    }

    fn ready_to_execute(&mut self, agent_id: u64, intent: &AgentIntent) -> Result<bool> {
        let entity = self.find_agent_entity(agent_id)?;
        let entry = self.world.entity(entity);
        let current_pos = entry
            .get::<PositionComponent>()
            .ok_or_else(|| anyhow!("missing position component"))?
            .0;
        match intent.kind {
            IntentKind::Socializar => {
                if let Some(target_id) = intent.target_agent {
                    self.agents_adjacent(agent_id, target_id)
                } else {
                    Ok(false)
                }
            }
            IntentKind::Comprar
            | IntentKind::Transportar
            | IntentKind::Vender
            | IntentKind::ReceberPagamento => Ok(entry
                .get::<DestinationComponent>()
                .ok_or_else(|| anyhow!("missing destination component"))?
                .0
                .map(|destination| destination == current_pos)
                .unwrap_or(false)),
            _ => Ok(entry
                .get::<DestinationComponent>()
                .ok_or_else(|| anyhow!("missing destination component"))?
                .0
                .map(|destination| destination == current_pos)
                .unwrap_or(false)),
        }
    }

    fn advance_agent_movement(&mut self, agent_id: u64) -> Result<bool> {
        let entity = self.find_agent_entity(agent_id)?;
        let (current_pos, path, name, active_conversation_id) = {
            let entry = self.world.entity(entity);
            (
                entry
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0,
                entry
                    .get::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0
                    .clone(),
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .name
                    .clone(),
                entry
                    .get::<ConversationComponent>()
                    .ok_or_else(|| anyhow!("missing conversation component"))?
                    .active_conversation_id,
            )
        };
        if active_conversation_id.is_some() {
            return Ok(false);
        }
        let Some(next_step) = path.first().copied() else {
            return Ok(false);
        };
        if !self.is_walkable(next_step) || self.is_occupied(next_step, Some(agent_id)) {
            self.increment_blocked_ticks(agent_id)?;
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Blocking,
                summary: format!("{name} fica bloqueado em seu caminho."),
                impact_tags: vec!["bloqueio".to_string(), "movimento".to_string()],
            });
            return Ok(false);
        }
        let previous_tile = self.tile_at(current_pos).cloned();
        {
            let mut entity_mut = self.world.entity_mut(entity);
            entity_mut
                .get_mut::<PositionComponent>()
                .ok_or_else(|| anyhow!("missing position component"))?
                .0 = next_step;
            let mut path_component = entity_mut
                .get_mut::<PathComponent>()
                .ok_or_else(|| anyhow!("missing path component"))?;
            if !path_component.0.is_empty() {
                path_component.0.remove(0);
            }
        }
        self.reset_blocked_ticks(agent_id)?;
        let new_tile = self.tile_at(next_step).cloned();
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::Travel,
            summary: format!("{name} anda para ({}, {}).", next_step.x, next_step.y),
            impact_tags: self.tile_tags(next_step),
        });

        if previous_tile.as_ref().and_then(|tile| tile.building_id)
            != new_tile.as_ref().and_then(|tile| tile.building_id)
        {
            if let Some(building_id) = new_tile.as_ref().and_then(|tile| tile.building_id) {
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: None,
                    kind: EventKind::Arrival,
                    summary: format!(
                        "{name} entra em {}.",
                        self.building_name(building_id)
                            .unwrap_or_else(|| "um edificio".to_string())
                    ),
                    impact_tags: vec!["entrada".to_string(), format!("building:{building_id}")],
                });
            }
        }

        let destination = self
            .world
            .entity(entity)
            .get::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0;
        if destination == Some(next_step) {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Arrival,
                summary: format!("{name} chega ao destino fisico atual."),
                impact_tags: self.tile_tags(next_step),
            });
        }

        Ok(true)
    }

    fn resolve_intent_candidates(
        &mut self,
        core: &AgentCore,
        current_pos: TileCoord,
        intent: &AgentIntent,
    ) -> Result<Vec<ResolvedTargetCandidate>> {
        let mut candidates = match intent.kind {
            IntentKind::Trabalhar => self.work_candidates(core.id, core),
            IntentKind::Descansar => self.rest_candidates(core),
            IntentKind::Comer => self.eat_candidates(core),
            IntentKind::Refletir => self.reflect_candidates(core),
            IntentKind::Andar => self.wander_candidates(core.id),
            IntentKind::Socializar => self.social_candidates(core.id, intent.target_agent),
            IntentKind::Comprar
            | IntentKind::Transportar
            | IntentKind::Vender
            | IntentKind::ReceberPagamento => self.economic_task_candidates(core.id),
        };
        candidates.sort_by_key(|candidate| current_pos.manhattan(candidate.destination));
        Ok(candidates)
    }

    fn work_candidates(&self, actor_id: u64, core: &AgentCore) -> Vec<ResolvedTargetCandidate> {
        let mut candidates = Vec::new();
        if let Some(task) = self.active_economic_task_for_agent(actor_id)
            && task.kind == EconomicTaskKind::Produzir
        {
            if let Some(establishment_id) = task.related_establishment_id
                && let Some(establishment) = self.establishment_by_id(establishment_id)
            {
                for fixture in self.spatial.fixtures.iter().filter(|fixture| {
                    fixture.kind == FixtureKind::Workstation
                        && fixture.building_id == establishment.building_id
                }) {
                    if let Some(destination) = self.fixture_access_tile(fixture) {
                        candidates.push(ResolvedTargetCandidate {
                            destination,
                            label: task.description.clone(),
                        });
                    }
                }
                if !candidates.is_empty() {
                    return candidates;
                }
            }
        }
        if core.role == Role::Farmer {
            for fixture in self.spatial.fixtures.iter().filter(|fixture| {
                fixture.kind == FixtureKind::Workstation
                    && self
                        .building_kind_opt(fixture.building_id)
                        .map(|kind| {
                            matches!(
                                kind,
                                LocationKind::Farm | LocationKind::Woodlot | LocationKind::Quarry
                            )
                        })
                        .unwrap_or(false)
            }) {
                if let Some(destination) = self.fixture_access_tile(fixture) {
                    candidates.push(ResolvedTargetCandidate {
                        destination,
                        label: format!("trabalho em {}", fixture.name),
                    });
                }
            }
            return candidates;
        }
        for fixture in self.spatial.fixtures.iter().filter(|fixture| {
            fixture.kind == FixtureKind::Workstation && fixture.building_id == core.work_building_id
        }) {
            if let Some(destination) = self.fixture_access_tile(fixture) {
                candidates.push(ResolvedTargetCandidate {
                    destination,
                    label: format!("trabalho em {}", fixture.name),
                });
            }
        }
        candidates
    }

    fn rest_candidates(&self, core: &AgentCore) -> Vec<ResolvedTargetCandidate> {
        let mut candidates = Vec::new();
        if let Some(home_bed) = core.home_bed {
            if let Some(destination) = self.access_tile_for_coord(home_bed) {
                candidates.push(ResolvedTargetCandidate {
                    destination,
                    label: "cama de casa".to_string(),
                });
            }
        }
        candidates
    }

    fn eat_candidates(&self, core: &AgentCore) -> Vec<ResolvedTargetCandidate> {
        let mut candidates = Vec::new();
        for fixture in self.spatial.fixtures.iter().filter(|fixture| {
            matches!(fixture.kind, FixtureKind::Table | FixtureKind::Seat)
                && (fixture.building_id == core.home_building_id
                    || self
                        .building_kind_opt(fixture.building_id)
                        .map(|kind| matches!(kind, LocationKind::Tavern | LocationKind::Bakery))
                        .unwrap_or(false))
        }) {
            if let Some(destination) = self.fixture_access_tile(fixture) {
                candidates.push(ResolvedTargetCandidate {
                    destination,
                    label: format!("comer perto de {}", fixture.name),
                });
            }
        }
        candidates
    }

    fn reflect_candidates(&self, core: &AgentCore) -> Vec<ResolvedTargetCandidate> {
        let mut candidates = Vec::new();
        for fixture in self.spatial.fixtures.iter().filter(|fixture| {
            matches!(fixture.kind, FixtureKind::Seat | FixtureKind::Table)
                && (fixture.building_id == core.home_building_id
                    || self
                        .building_kind_opt(fixture.building_id)
                        .map(|kind| kind == LocationKind::Tavern)
                        .unwrap_or(false))
        }) {
            if let Some(destination) = self.fixture_access_tile(fixture) {
                candidates.push(ResolvedTargetCandidate {
                    destination,
                    label: format!("refletir perto de {}", fixture.name),
                });
            }
        }
        for coord in [
            TileCoord { x: 24, y: 13 },
            TileCoord { x: 22, y: 13 },
            TileCoord { x: 26, y: 13 },
        ] {
            candidates.push(ResolvedTargetCandidate {
                destination: coord,
                label: "praca central".to_string(),
            });
        }
        candidates
    }

    fn wander_candidates(&self, actor_id: u64) -> Vec<ResolvedTargetCandidate> {
        let plaza = [
            TileCoord { x: 24, y: 13 },
            TileCoord { x: 21, y: 13 },
            TileCoord { x: 27, y: 13 },
            TileCoord { x: 24, y: 15 },
        ];
        let index = (self.total_ticks as usize + actor_id as usize) % plaza.len();
        vec![ResolvedTargetCandidate {
            destination: plaza[index],
            label: "praca central".to_string(),
        }]
    }

    fn social_candidates(
        &mut self,
        actor_id: u64,
        target_agent: Option<u64>,
    ) -> Vec<ResolvedTargetCandidate> {
        let Some(target_agent) = target_agent else {
            return self.wander_candidates(actor_id);
        };
        let Ok(target_pos) = self.debug_agent_position(target_agent) else {
            return self.wander_candidates(actor_id);
        };
        let mut candidates = Vec::new();
        for neighbor in target_pos.neighbors4() {
            if self.is_walkable(neighbor) && !self.is_occupied(neighbor, Some(actor_id)) {
                candidates.push(ResolvedTargetCandidate {
                    destination: neighbor,
                    label: format!("aproximar-se de agente {}", target_agent),
                });
            }
        }
        candidates
    }

    fn economic_task_candidates(&mut self, agent_id: u64) -> Vec<ResolvedTargetCandidate> {
        let Some(task) = self.active_economic_task_for_agent(agent_id).cloned() else {
            return Vec::new();
        };
        let node = match task.phase {
            EconomicTaskPhase::AwaitingPickup => &task.source,
            EconomicTaskPhase::InTransit | EconomicTaskPhase::AwaitingPayment => &task.destination,
            EconomicTaskPhase::Completed | EconomicTaskPhase::Failed => return Vec::new(),
        };
        self.node_access_tile(node)
            .map(|destination| {
                vec![ResolvedTargetCandidate {
                    destination,
                    label: task.description,
                }]
            })
            .unwrap_or_default()
    }

    fn active_economic_task_for_agent(&self, agent_id: u64) -> Option<&EconomicTask> {
        self.economic_tasks.iter().find(|task| {
            task.assigned_agent_id == Some(agent_id)
                && task.phase != EconomicTaskPhase::Completed
                && task.phase != EconomicTaskPhase::Failed
        })
    }

    fn node_access_tile(&self, node: &EconomicNode) -> Option<TileCoord> {
        match node {
            EconomicNode::HouseholdPantry(building_id) => self
                .nearest_storage_for_building(Some(*building_id))
                .and_then(|fixture_id| self.fixture_by_id(fixture_id))
                .and_then(|fixture| self.fixture_access_tile(fixture))
                .or_else(|| self.building_by_id(*building_id).map(|building| building.entrance)),
            EconomicNode::Establishment(establishment_id) => self
                .establishment_by_id(*establishment_id)
                .and_then(|establishment| establishment.storage_fixture_id)
                .and_then(|fixture_id| self.fixture_by_id(fixture_id))
                .and_then(|fixture| self.fixture_access_tile(fixture))
                .or_else(|| {
                    self.establishment_by_id(*establishment_id)
                        .and_then(|establishment| establishment.building_id)
                        .and_then(|building_id| self.building_by_id(building_id))
                        .map(|building| building.entrance)
                }),
            EconomicNode::ExternalMarket => Some(self.village_economy.external_market_coord),
            EconomicNode::PublicTreasury => self
                .spatial
                .buildings
                .iter()
                .find(|building| building.kind == LocationKind::Manor)
                .map(|building| building.entrance),
        }
    }

    fn remove_resource_from_node(
        &mut self,
        node: &EconomicNode,
        resource: ResourceKind,
        amount: i32,
    ) -> i32 {
        match node {
            EconomicNode::HouseholdPantry(building_id) => self
                .household_by_id_mut(*building_id)
                .map(|household| Self::take_resource(&mut household.pantry, resource, amount))
                .unwrap_or(0),
            EconomicNode::Establishment(establishment_id) => self
                .establishment_by_id_mut(*establishment_id)
                .map(|establishment| Self::take_resource(&mut establishment.stock, resource, amount))
                .unwrap_or(0),
            EconomicNode::ExternalMarket => amount.max(0),
            EconomicNode::PublicTreasury => 0,
        }
    }

    fn add_resource_to_node(&mut self, node: &EconomicNode, resource: ResourceKind, amount: i32) {
        match node {
            EconomicNode::HouseholdPantry(building_id) => {
                if let Some(household) = self.household_by_id_mut(*building_id) {
                    Self::push_resource(&mut household.pantry, resource, amount);
                }
            }
            EconomicNode::Establishment(establishment_id) => {
                if let Some(establishment) = self.establishment_by_id_mut(*establishment_id) {
                    Self::push_resource(&mut establishment.stock, resource, amount);
                }
            }
            EconomicNode::ExternalMarket | EconomicNode::PublicTreasury => {}
        }
    }

    fn withdraw_cash_for_purchase(&mut self, task: &EconomicTask) -> bool {
        let total_price = task.total_price.max(0);
        if total_price == 0 {
            return true;
        }
        match task.destination {
            EconomicNode::HouseholdPantry(household_id) => {
                if let Some(household) = self.household_by_id_mut(household_id)
                    && household.treasury >= total_price
                {
                    household.treasury -= total_price;
                    return true;
                }
            }
            EconomicNode::Establishment(establishment_id) => {
                if let Some(establishment) = self.establishment_by_id_mut(establishment_id)
                    && establishment.cash >= total_price
                {
                    establishment.cash -= total_price;
                    return true;
                }
            }
            EconomicNode::ExternalMarket | EconomicNode::PublicTreasury => {}
        }
        false
    }

    fn deposit_cash_to_sale_target(&mut self, task: &EconomicTask) {
        if task.total_price <= 0 {
            return;
        }
        match task.kind {
            EconomicTaskKind::Produzir => {}
            EconomicTaskKind::Comprar => {
                if let EconomicNode::Establishment(source_id) = task.source
                    && let Some(establishment) = self.establishment_by_id_mut(source_id)
                {
                    establishment.cash += task.total_price;
                }
            }
            EconomicTaskKind::Vender => {
                if let Some(establishment_id) = task.related_establishment_id
                    && let Some(establishment) = self.establishment_by_id_mut(establishment_id)
                {
                    establishment.cash += task.total_price;
                } else if let Some(household) = self.household_by_id_mut(task.actor_household_id) {
                    household.treasury += task.total_price;
                }
            }
            EconomicTaskKind::Transportar | EconomicTaskKind::ReceberPagamento => {}
        }
    }

    fn apply_economic_intent(&mut self, agent_id: u64) -> Result<()> {
        let Some(task) = self.active_economic_task_for_agent(agent_id).cloned() else {
            self.clear_intent_navigation(agent_id)?;
            return Ok(());
        };
        match task.kind {
            EconomicTaskKind::ReceberPagamento => self.execute_payment_collection(agent_id, task),
            _ => self.execute_logistics_task(agent_id, task),
        }
    }

    fn execute_payment_collection(&mut self, agent_id: u64, task: EconomicTask) -> Result<()> {
        let collected = self.collect_pending_payments(task.actor_household_id);
        let agent_name = self.agent_name(agent_id)?;
        let entity = self.find_agent_entity(agent_id)?;
        if let Some(mut economic) = self
            .world
            .entity_mut(entity)
            .get_mut::<EconomicActivityComponent>()
        {
            economic.active_task_id = None;
        }
        if let Some(task_state) = self.economic_tasks.iter_mut().find(|entry| entry.id == task.id) {
            task_state.phase = EconomicTaskPhase::Completed;
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::Salary,
            summary: format!("{agent_name} recolhe {collected} moeda(s) em pagamentos."),
            impact_tags: vec!["salario".to_string(), "pagamento".to_string()],
        });
        Ok(())
    }

    fn execute_logistics_task(&mut self, agent_id: u64, task: EconomicTask) -> Result<()> {
        let resource = task
            .resource
            .ok_or_else(|| anyhow!("economic task {} missing resource", task.id))?;
        match task.phase {
            EconomicTaskPhase::AwaitingPickup => {
                let agent_name = self.agent_name(agent_id)?;
                if task.kind == EconomicTaskKind::Comprar && !self.withdraw_cash_for_purchase(&task) {
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: agent_id,
                        target: None,
                        kind: EventKind::Scarcity,
                        summary: format!("{agent_name} nao tem caixa suficiente para {}.", task.description),
                        impact_tags: vec!["escassez".to_string(), "caixa".to_string()],
                    });
                    return Ok(());
                }
                let amount = self.remove_resource_from_node(&task.source, resource, task.amount);
                if amount <= 0 {
                    if let Some(task_state) = self.economic_tasks.iter_mut().find(|entry| entry.id == task.id) {
                        task_state.phase = EconomicTaskPhase::Failed;
                    }
                    return Ok(());
                }
                let entity = self.find_agent_entity(agent_id)?;
                self.world
                    .entity_mut(entity)
                    .get_mut::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?
                    .carrying = vec![ResourceStack { kind: resource, amount }];
                if let Some(task_state) = self.economic_tasks.iter_mut().find(|entry| entry.id == task.id) {
                    task_state.phase = EconomicTaskPhase::InTransit;
                    task_state.amount = amount;
                    task_state.total_price = task_state.unit_price * amount;
                }
                self.deposit_cash_to_sale_target(&task);
                self.sync_establishment_stocks_to_fixtures();
            }
            EconomicTaskPhase::InTransit => {
                let agent_name = self.agent_name(agent_id)?;
                let carried_amount = {
                    let entity = self.find_agent_entity(agent_id)?;
                    let entry = self.world.entity(entity);
                    entry.get::<EconomicActivityComponent>()
                        .ok_or_else(|| anyhow!("missing economy component"))?
                        .carrying
                        .iter()
                        .find(|stack| stack.kind == resource)
                        .map(|stack| stack.amount)
                        .unwrap_or(0)
                };
                if carried_amount > 0 {
                    self.add_resource_to_node(&task.destination, resource, carried_amount);
                }
                let entity = self.find_agent_entity(agent_id)?;
                let mut entity_mut = self.world.entity_mut(entity);
                let mut economic = entity_mut
                    .get_mut::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?;
                economic.carrying.clear();
                economic.active_task_id = None;
                if let Some(task_state) = self.economic_tasks.iter_mut().find(|entry| entry.id == task.id) {
                    task_state.phase = EconomicTaskPhase::Completed;
                }
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: None,
                    kind: EventKind::Logistics,
                    summary: format!("{agent_name} conclui a tarefa: {}.", task.description),
                    impact_tags: vec!["logistica".to_string(), resource.as_str().to_string()],
                });
                self.sync_household_pantries_to_fixtures();
                self.sync_establishment_stocks_to_fixtures();
            }
            EconomicTaskPhase::AwaitingPayment
            | EconomicTaskPhase::Completed
            | EconomicTaskPhase::Failed => {}
        }
        Ok(())
    }

    fn collect_pending_payments(&mut self, household_id: BuildingId) -> i32 {
        let claims = self
            .household_by_id(household_id)
            .map(|household| household.pending_payments.clone())
            .unwrap_or_default();
        let actor_id = self
            .household_by_id(household_id)
            .and_then(|household| household.member_ids.first().copied())
            .unwrap_or(0);
        let mut collected = 0;
        for claim in claims {
            let paid = if let Some(establishment_id) = claim.payer_establishment_id {
                if let Some(establishment) = self.establishment_by_id_mut(establishment_id) {
                    let amount = establishment.cash.min(claim.amount);
                    establishment.cash -= amount;
                    amount
                } else {
                    0
                }
            } else {
                let amount = self.village_economy.public_treasury.min(claim.amount);
                self.village_economy.public_treasury -= amount;
                amount
            };
            if paid > 0 {
                collected += paid;
                if let Some(household) = self.household_by_id_mut(household_id) {
                    household.treasury += paid;
                    if let Some(existing) = household
                        .pending_payments
                        .iter_mut()
                        .find(|pending| pending.payer_label == claim.payer_label && pending.amount == claim.amount)
                    {
                        existing.amount -= paid;
                    }
                    household.pending_payments.retain(|pending| pending.amount > 0);
                }
            }
            if paid < claim.amount {
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: actor_id,
                    target: None,
                    kind: EventKind::Salary,
                    summary: format!(
                        "Pagamento de {} ficou parcial: {}/{} moeda(s).",
                        claim.payer_label, paid, claim.amount
                    ),
                    impact_tags: vec!["salario".to_string(), "atraso".to_string()],
                });
            }
        }
        collected
    }

    fn apply_work(&mut self, actor_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(actor_id)?;
        let active_production_task = self
            .active_economic_task_for_agent(actor_id)
            .filter(|task| task.kind == EconomicTaskKind::Produzir)
            .cloned();
        let (name, role, work_building_id, home_building_id) = {
            let entry = self.world.entity(entity);
            let core = entry
                .get::<AgentCore>()
                .ok_or_else(|| anyhow!("missing agent core"))?;
            (
                core.name.clone(),
                core.role,
                active_production_task
                    .as_ref()
                    .and_then(|task| task.related_establishment_id)
                    .or(core.work_building_id),
                core.home_building_id,
            )
        };
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.energy = (state.0.energy - 10).clamp(0, 100);
            state.0.hunger = (state.0.hunger + 10).clamp(0, 100);
            state.0.stress = (state.0.stress + 4).clamp(0, 100);
            state.0.mood = (state.0.mood + 1).clamp(0, 100);
        }
        let mut produced = ResourceStack {
            kind: ResourceKind::Moedas,
            amount: 0,
        };
        let mut work_failed_reason = None::<String>;
        let mut salary_claim = None::<PendingPaymentClaim>;
        if let Some(building_id) = work_building_id {
            if let Some(establishment) = self.establishment_by_building_mut(building_id) {
                let can_work = match role {
                    Role::Farmer => {
                        let produced_kind = active_production_task
                            .as_ref()
                            .and_then(|task| task.resource)
                            .unwrap_or_else(|| match establishment.kind {
                                LocationKind::Woodlot => ResourceKind::Lenha,
                                LocationKind::Quarry => ResourceKind::MetalBruto,
                                _ => ResourceKind::Graos,
                            });
                        match produced_kind {
                            ResourceKind::Graos => {
                                let tools = Self::total_resource_amount(
                                    &establishment.stock,
                                    ResourceKind::Ferramentas,
                                );
                                if tools <= 0 {
                                    work_failed_reason =
                                        Some("faltam ferramentas no campo".to_string());
                                    false
                                } else {
                                    produced = ResourceStack {
                                        kind: ResourceKind::Graos,
                                        amount: 4,
                                    };
                                    establishment.tool_wear += 1;
                                    if establishment.tool_wear >= 4 {
                                        Self::take_resource(
                                            &mut establishment.stock,
                                            ResourceKind::Ferramentas,
                                            1,
                                        );
                                        establishment.tool_wear = 0;
                                    }
                                    true
                                }
                            }
                            ResourceKind::Lenha => {
                                produced = ResourceStack {
                                    kind: ResourceKind::Lenha,
                                    amount: 3,
                                };
                                true
                            }
                            ResourceKind::MetalBruto => {
                                produced = ResourceStack {
                                    kind: ResourceKind::MetalBruto,
                                    amount: 2,
                                };
                                true
                            }
                            _ => false,
                        }
                    }
                    Role::Blacksmith => {
                        let metal = Self::take_resource(&mut establishment.stock, ResourceKind::MetalBruto, 1);
                        let wood = Self::take_resource(&mut establishment.stock, ResourceKind::Lenha, 1);
                        if metal < 1 || wood < 1 {
                            if metal > 0 {
                                Self::push_resource(&mut establishment.stock, ResourceKind::MetalBruto, metal);
                            }
                            if wood > 0 {
                                Self::push_resource(&mut establishment.stock, ResourceKind::Lenha, wood);
                            }
                            work_failed_reason = Some("faltam insumos na forja".to_string());
                            false
                        } else {
                            produced = ResourceStack {
                                kind: ResourceKind::Ferramentas,
                                amount: 1,
                            };
                            true
                        }
                    }
                    Role::Baker => {
                        let grains = Self::take_resource(&mut establishment.stock, ResourceKind::Graos, 2);
                        let wood = Self::take_resource(&mut establishment.stock, ResourceKind::Lenha, 1);
                        if grains < 2 || wood < 1 {
                            if grains > 0 {
                                Self::push_resource(&mut establishment.stock, ResourceKind::Graos, grains);
                            }
                            if wood > 0 {
                                Self::push_resource(&mut establishment.stock, ResourceKind::Lenha, wood);
                            }
                            work_failed_reason = Some("faltam graos ou lenha na padaria".to_string());
                            false
                        } else {
                            produced = ResourceStack {
                                kind: ResourceKind::Pao,
                                amount: 3,
                            };
                            true
                        }
                    }
                    Role::TavernKeeper => {
                        let grains = Self::take_resource(&mut establishment.stock, ResourceKind::Graos, 1);
                        let wood = Self::take_resource(&mut establishment.stock, ResourceKind::Lenha, 1);
                        if grains < 1 || wood < 1 {
                            if grains > 0 {
                                Self::push_resource(&mut establishment.stock, ResourceKind::Graos, grains);
                            }
                            if wood > 0 {
                                Self::push_resource(&mut establishment.stock, ResourceKind::Lenha, wood);
                            }
                            work_failed_reason = Some("faltam insumos na taverna".to_string());
                            false
                        } else {
                            produced = ResourceStack {
                                kind: ResourceKind::Caldo,
                                amount: 2,
                            };
                            true
                        }
                    }
                    Role::Guard | Role::Headman => true,
                };
                if can_work {
                    if produced.amount > 0 {
                        Self::push_resource(&mut establishment.stock, produced.kind, produced.amount);
                    }
                    if let Some(_household_id) = home_building_id {
                        salary_claim = Some(PendingPaymentClaim {
                            payer_establishment_id: if establishment.public_service {
                                None
                            } else {
                                Some(establishment.id)
                            },
                            payer_label: establishment.name.clone(),
                            amount: establishment.wage_per_shift,
                        });
                    }
                }
            }
        }
        let had_salary_claim = salary_claim.is_some();
        if let Some(household_id) = home_building_id
            && let Some(claim) = salary_claim
            && let Some(household) = self.household_by_id_mut(household_id)
        {
            household.pending_payments.push(claim);
        }
        self.sync_establishment_stocks_to_fixtures();
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: if work_failed_reason.is_some() {
                EventKind::Scarcity
            } else if had_salary_claim {
                EventKind::Salary
            } else {
                EventKind::Routine
            },
            summary: if let Some(reason) = work_failed_reason.clone() {
                format!("{name} tenta trabalhar como {}, mas {}.", role.as_str(), reason)
            } else {
                format!("{name} trabalha como {}.", role.as_str())
            },
            impact_tags: vec!["trabalho".to_string(), produced.kind.as_str().to_string()],
        });
        if work_failed_reason.is_none() {
            if let Some(task) = active_production_task
                && let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
            {
                task_state.phase = EconomicTaskPhase::Completed;
            }
            self.clear_active_economic_task(actor_id)?;
            self.add_memory(
                actor_id,
                MemoryKind::Success,
                if produced.amount > 0 {
                    format!("Trabalho concluido produzindo {}.", produced.kind.as_str())
                } else {
                    "Trabalho civico concluido e pagamento aguardado.".to_string()
                },
                vec!["trabalho".to_string(), produced.kind.as_str().to_string()],
                8,
                Vec::new(),
            )?;
        }
        Ok(())
    }

    fn apply_rest(&mut self, actor_id: u64) -> Result<()> {
        let name = self.agent_name(actor_id)?;
        let entity = self.find_agent_entity(actor_id)?;
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.energy = (state.0.energy + 18).clamp(0, 100);
            state.0.stress = (state.0.stress - 10).clamp(0, 100);
            state.0.mood = (state.0.mood + 3).clamp(0, 100);
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: EventKind::Routine,
            summary: format!("{name} descansa perto de sua cama."),
            impact_tags: vec!["descanso".to_string()],
        });
        Ok(())
    }

    fn apply_eat(&mut self, actor_id: u64) -> Result<()> {
        let name = self.agent_name(actor_id)?;
        let entity = self.find_agent_entity(actor_id)?;
        let ate = self.consume_food_for_agent(actor_id)?;
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            if ate {
                state.0.hunger = (state.0.hunger - 28).clamp(0, 100);
                state.0.stress = (state.0.stress - 4).clamp(0, 100);
                state.0.mood = (state.0.mood + 4).clamp(0, 100);
            } else {
                state.0.stress = (state.0.stress + 6).clamp(0, 100);
                state.0.mood = (state.0.mood - 4).clamp(0, 100);
            }
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: if ate {
                EventKind::Commerce
            } else {
                EventKind::Need
            },
            summary: if ate {
                format!("{name} come e recupera forcas.")
            } else {
                format!("{name} procura comida, mas encontra escassez.")
            },
            impact_tags: vec!["fome".to_string()],
        });
        Ok(())
    }

    fn apply_reflect(&mut self, actor_id: u64) -> Result<()> {
        let name = self.agent_name(actor_id)?;
        let entity = self.find_agent_entity(actor_id)?;
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.stress = (state.0.stress - 8).clamp(0, 100);
            state.0.mood = (state.0.mood + 2).clamp(0, 100);
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: EventKind::Reflection,
            summary: format!("{name} se recolhe para refletir em um lugar calmo."),
            impact_tags: vec!["reflexao".to_string()],
        });
        Ok(())
    }

    fn apply_wander(&mut self, actor_id: u64) -> Result<()> {
        let name = self.agent_name(actor_id)?;
        let entity = self.find_agent_entity(actor_id)?;
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.stress = (state.0.stress - 1).clamp(0, 100);
            state.0.mood = (state.0.mood + 1).clamp(0, 100);
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: EventKind::Travel,
            summary: format!("{name} passeia pela vila."),
            impact_tags: vec!["movimento".to_string()],
        });
        Ok(())
    }

    fn open_conversation(
        &mut self,
        initiator_id: u64,
        partner_id: u64,
        move_kind: SocialMove,
        reason: &str,
    ) -> Result<bool> {
        if !self.agents_adjacent(initiator_id, partner_id)? {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: initiator_id,
                target: Some(partner_id),
                kind: EventKind::Blocking,
                summary: "A conversa falha por falta de proximidade fisica.".to_string(),
                impact_tags: vec!["social".to_string(), "distancia".to_string()],
            });
            return Ok(false);
        }
        if self.agent_conversation_id(initiator_id)?.is_some()
            || self.agent_conversation_id(partner_id)?.is_some()
        {
            return Ok(false);
        }
        if self.agent_social_cooldown_until(initiator_id)? > self.total_ticks
            || self.agent_social_cooldown_until(partner_id)? > self.total_ticks
        {
            return Ok(false);
        }

        let conversation_id = self.next_conversation_id;
        self.next_conversation_id += 1;
        let initiator_name = self.agent_name(initiator_id)?;
        let partner_name = self.agent_name(partner_id)?;
        let opening_reason = format!("{}: {}", move_kind.as_str(), reason);
        self.conversations.push(ConversationState {
            id: conversation_id,
            participants: [initiator_id, partner_id],
            initiator_id,
            current_speaker_id: initiator_id,
            started_at_tick: self.total_ticks,
            turn_count: 0,
            max_turns: MAX_CONVERSATION_TURNS,
            opening_reason: opening_reason.clone(),
            summary: format!("{initiator_name} inicia uma conversa com {partner_name}."),
            recent_turns: Vec::new(),
            participant_states: vec![
                ConversationParticipantState {
                    agent_id: initiator_id,
                    social_goal: social_goal_from_move(move_kind).to_string(),
                    last_speech_act: None,
                    last_emotion: None,
                },
                ConversationParticipantState {
                    agent_id: partner_id,
                    social_goal: "entender a intencao do outro".to_string(),
                    last_speech_act: None,
                    last_emotion: None,
                },
            ],
            status: ConversationStatus::Active,
            outcome: ConversationOutcome::Ongoing,
            end_reason: None,
        });

        self.bind_agent_to_conversation(
            initiator_id,
            conversation_id,
            partner_id,
            format!("abre conversa para {}", move_kind.as_str()),
        )?;
        self.bind_agent_to_conversation(
            partner_id,
            conversation_id,
            initiator_id,
            "aceita conversa".to_string(),
        )?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: initiator_id,
            target: Some(partner_id),
            kind: EventKind::ConversationStarted,
            summary: format!("{initiator_name} inicia conversa com {partner_name}: {reason}."),
            impact_tags: vec![
                "social".to_string(),
                "conversa".to_string(),
                move_kind.as_str().to_string(),
            ],
        });
        Ok(true)
    }

    fn process_active_conversations(&mut self, llm: &dyn LlmAdapter) -> Result<()> {
        let active_ids = self
            .conversations
            .iter()
            .filter(|conversation| conversation.status == ConversationStatus::Active)
            .map(|conversation| conversation.id)
            .collect::<Vec<_>>();
        let mut prepared_turns = Vec::new();
        for conversation_id in active_ids {
            let Some(conversation) = self.conversation_state(conversation_id) else {
                continue;
            };
            if conversation.status != ConversationStatus::Active {
                continue;
            }
            if let Some((status, outcome, reason)) =
                self.conversation_interruption(&conversation)?
            {
                self.end_conversation(conversation_id, status, outcome, reason)?;
                continue;
            }

            let speaker_id = conversation.current_speaker_id;
            let listener_id = other_participant(&conversation.participants, speaker_id);
            let input =
                self.build_conversation_turn_input(&conversation, speaker_id, listener_id)?;
            prepared_turns.push(PreparedConversationTurn {
                conversation_id,
                speaker_id,
                listener_id,
                input,
            });
        }
        if prepared_turns.is_empty() {
            return Ok(());
        }

        let turn_results = self.run_parallel_conversation_turns(llm, prepared_turns)?;
        for result in turn_results {
            match result {
                ConversationBatchItem::Completed(result) => {
                    self.apply_conversation_turn_output(
                        result.conversation_id,
                        result.speaker_id,
                        result.listener_id,
                        result.output,
                    )?;
                }
                ConversationBatchItem::Interrupted(result) => {
                    self.handle_transient_conversation_failure(
                        result.conversation_id,
                        result.speaker_id,
                        result.listener_id,
                        &result.error,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn conversation_interruption(
        &mut self,
        conversation: &ConversationState,
    ) -> Result<Option<(ConversationStatus, ConversationOutcome, String)>> {
        let [agent_a, agent_b] = conversation.participants;
        if !self.agents_adjacent(agent_a, agent_b)? {
            return Ok(Some((
                ConversationStatus::Interrupted,
                ConversationOutcome::DistanceBreak,
                "os participantes perderam adjacencia".to_string(),
            )));
        }
        for agent_id in conversation.participants {
            let state = self.agent_state(agent_id)?;
            if state.hunger >= 95 || state.energy <= 5 || state.health <= 15 {
                return Ok(Some((
                    ConversationStatus::Interrupted,
                    ConversationOutcome::CriticalNeed,
                    format!(
                        "{} abandona a conversa por necessidade critica.",
                        self.agent_name(agent_id)?
                    ),
                )));
            }
        }
        Ok(None)
    }

    fn build_conversation_turn_input(
        &mut self,
        conversation: &ConversationState,
        speaker_id: u64,
        listener_id: u64,
    ) -> Result<ConversationTurnInput> {
        let speaker_entity = self.find_agent_entity(speaker_id)?;
        let listener_entity = self.find_agent_entity(listener_id)?;
        let (
            speaker_name,
            speaker_role,
            speaker_position,
            speaker_state,
            speaker_profile,
            speaker_memories,
        ) = {
            let entry = self.world.entity(speaker_entity);
            (
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .name
                    .clone(),
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .role,
                entry
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0,
                entry
                    .get::<StateComponent>()
                    .ok_or_else(|| anyhow!("missing state component"))?
                    .0
                    .clone(),
                entry
                    .get::<ProfileComponent>()
                    .ok_or_else(|| anyhow!("missing profile component"))?
                    .0
                    .clone(),
                entry
                    .get::<MemoryComponent>()
                    .ok_or_else(|| anyhow!("missing memory component"))?
                    .0
                    .clone(),
            )
        };
        let (listener_name, listener_role, listener_state) = {
            let entry = self.world.entity(listener_entity);
            (
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .name
                    .clone(),
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .role,
                entry
                    .get::<StateComponent>()
                    .ok_or_else(|| anyhow!("missing state component"))?
                    .0
                    .clone(),
            )
        };
        let recent_events =
            self.recent_events_for(speaker_id, speaker_position, self.recent_event_limit);
        let recent_memories = retrieve_relational_memories(&speaker_memories, listener_id, 5);
        let tile = self.tile_at(speaker_position);
        let current_building = tile
            .and_then(|entry| entry.building_id)
            .and_then(|id| self.building_name(id));
        let current_room = tile
            .and_then(|entry| entry.room_id)
            .and_then(|id| self.room_name(id));
        let agent_name_map = self.agent_name_map();
        let recent_turns = conversation
            .recent_turns
            .iter()
            .map(|turn| {
                format!(
                    "{} [{}]: {}",
                    agent_name_map
                        .get(&turn.speaker_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Agente {}", turn.speaker_id)),
                    turn.speech_act,
                    turn.utterance
                )
            })
            .collect::<Vec<_>>();
        let relation = self.relation_between(speaker_id, listener_id);
        let speaker_psychology = self.build_psychological_context_for_values(
            speaker_id,
            &speaker_profile,
            &speaker_state,
            &speaker_memories,
            &recent_events,
            &recent_memories,
            "conversa_ativa",
        );
        let listener_memories = self.agent_memories(listener_id)?;
        let listener_recent_events =
            self.recent_events_for(listener_id, speaker_position, self.recent_event_limit);
        let listener_relevant_memories =
            retrieve_relational_memories(&listener_memories, speaker_id, 5);
        let listener_profile = self.agent_profile(listener_id)?;
        let listener_psychology = self.build_psychological_context_for_values(
            listener_id,
            &listener_profile,
            &listener_state,
            &listener_memories,
            &listener_recent_events,
            &listener_relevant_memories,
            "observando_conversa",
        );
        let relational_context =
            self.build_relational_history(speaker_id, listener_id, &relation, &speaker_memories);

        Ok(ConversationTurnInput {
            speaker_id,
            speaker_name,
            speaker_role: speaker_role.as_str().to_string(),
            speaker_state,
            speaker_profile_summary: {
                let mut summary = speaker_profile.values.clone();
                summary.extend(speaker_profile.long_term_desires.clone());
                summary.extend(speaker_profile.fears.clone());
                summary
            },
            speaker_psychology,
            listener: ConversationObservedAgentInput {
                id: listener_id,
                name: listener_name,
                role: listener_role.as_str().to_string(),
                state: listener_state,
                relation,
                psychological_summary: listener_psychology,
            },
            context: ConversationContextInput {
                conversation_id: conversation.id,
                opening_reason: conversation.opening_reason.clone(),
                current_area: self.area_name(speaker_position),
                current_building,
                current_room,
                max_turns: conversation.max_turns,
                turn_count: conversation.turn_count,
                turns_remaining: conversation
                    .max_turns
                    .saturating_sub(conversation.turn_count),
                conversation_summary: conversation.summary.clone(),
            },
            turn_trigger: "fala_social".to_string(),
            relational_context,
            recent_memories,
            recent_turns: recent_turns
                .into_iter()
                .rev()
                .take(CONVERSATION_RECENT_TURNS_LIMIT)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
        })
    }

    fn process_general_decisions(&mut self, llm: &dyn LlmAdapter) -> Result<()> {
        let requests = self.prepare_decision_requests()?;
        if requests.is_empty() {
            return Ok(());
        }

        let decision_results = self.run_parallel_decisions(llm, requests)?;
        for result in decision_results {
            match result {
                DecisionBatchItem::Completed(result) => {
                    let intent = validate_intent(result.envelope.intent, &result.nearby_ids);
                    self.assign_intent(result.agent_id, intent, result.envelope.reflection)?;
                    self.record_cognition_trigger(result.agent_id, &result.cognition_trigger)?;
                    self.record_social_opportunity_signature(
                        result.agent_id,
                        result.social_opportunity_signature,
                    )?;
                    self.ensure_navigation_for_current_intent(result.agent_id)?;
                    self.try_execute_current_intent(result.agent_id, llm)?;
                }
                DecisionBatchItem::Skipped(result) => {
                    self.handle_transient_decision_failure(
                        result.agent_id,
                        &result.cognition_trigger,
                        result.social_opportunity_signature,
                        &result.error,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn prepare_decision_requests(&mut self) -> Result<Vec<PreparedDecisionRequest>> {
        let contexts = self.collect_contexts();
        let mut requests = Vec::new();

        for context in contexts {
            if context.active_conversation_id.is_some() {
                continue;
            }
            if context.social_cooldown_until > self.total_ticks
                && matches!(
                    context.last_intent.as_ref().map(|intent| intent.kind),
                    Some(IntentKind::Socializar)
                )
            {
                continue;
            }

            let recent_events = self.recent_events_for(
                context.id,
                context.position,
                self.recent_event_limit.max(8),
            );
            let social_opportunity_signature = self.social_opportunity_signature(&context);
            let Some(cognition_trigger) =
                self.decision_trigger_for_context(&context, &recent_events)?
            else {
                continue;
            };

            let context_depth = self
                .context_depth_for_trigger(&cognition_trigger)
                .to_string();
            let (memory_limit, fixture_limit, agent_limit, event_limit) =
                self.context_limits_for_trigger(&cognition_trigger);
            let relevant_memories = retrieve_relevant_memories(
                &context.memories,
                &context.state,
                &recent_events,
                memory_limit,
            );
            let mut nearby_agents = self.nearby_agent_inputs(
                context.id,
                context.position,
                context.current_room_id,
                &context.relations,
            );
            nearby_agents.truncate(agent_limit);
            let nearby_ids = nearby_agents.iter().map(|item| item.id).collect::<Vec<_>>();
            let mut nearby_fixtures = self.nearby_fixture_inputs(context.position, 6);
            nearby_fixtures.truncate(fixture_limit);
            let psychological_context = self.build_psychological_context(
                &context,
                &recent_events,
                &relevant_memories,
                &cognition_trigger,
            );
            let economic_context = self.build_economic_context(&context);
            let input = DecisionInput {
                actor_id: context.id,
                actor_name: context.name.clone(),
                role: context.role.as_str().to_string(),
                day: self.day,
                tick: self.tick_of_day,
                current_area: self.area_name(context.position),
                current_building: context
                    .current_building_id
                    .and_then(|id| self.building_name(id)),
                current_building_kind: context
                    .current_building_id
                    .and_then(|id| self.building_kind(id).map(|kind| kind.as_str().to_string())),
                current_room: context.current_room_id.and_then(|id| self.room_name(id)),
                accessible_exits: self.accessible_exits(context.position),
                nearby_fixtures,
                nearby_agents,
                relevant_memories,
                recent_events: recent_events
                    .into_iter()
                    .take(event_limit)
                    .map(|event| RecentEventInput {
                        day: event.day,
                        tick: event.tick,
                        kind: event.kind,
                        summary: event.summary,
                    })
                    .collect(),
                current_goals: context.state.active_goals.clone(),
                known_destination: context.destination_label.clone(),
                blockers: self.local_blockers(context.position),
                state: context.state.clone(),
                cognition_trigger: cognition_trigger.clone(),
                context_depth,
                psychological_context,
                economic_context,
                profile_summary: context.profile_summary(),
                llm_budget_remaining: 24u32.saturating_sub(context.llm_calls as u32),
            };
            requests.push(PreparedDecisionRequest {
                agent_id: context.id,
                nearby_ids,
                cognition_trigger,
                social_opportunity_signature,
                input,
            });
        }

        requests.sort_by_key(|request| request.agent_id);
        Ok(requests)
    }

    fn run_parallel_decisions(
        &self,
        llm: &dyn LlmAdapter,
        requests: Vec<PreparedDecisionRequest>,
    ) -> Result<Vec<DecisionBatchItem>> {
        thread::scope(|scope| -> Result<Vec<DecisionBatchItem>> {
            let mut handles = Vec::with_capacity(requests.len());
            for request in requests {
                handles.push(scope.spawn(move || {
                    let PreparedDecisionRequest {
                        agent_id,
                        nearby_ids,
                        cognition_trigger,
                        social_opportunity_signature,
                        input,
                    } = request;
                    match llm.evaluate_and_decide(&input) {
                        Ok(envelope) => {
                            DecisionWorkerResult::Completed(CompletedDecisionRequest {
                                agent_id,
                                nearby_ids,
                                cognition_trigger,
                                social_opportunity_signature,
                                envelope,
                            })
                        }
                        Err(error) => DecisionWorkerResult::Skipped(SkippedDecisionRequest {
                            agent_id,
                            cognition_trigger,
                            social_opportunity_signature,
                            error,
                        }),
                    }
                }));
            }

            let mut results = Vec::with_capacity(handles.len());
            for handle in handles {
                let result = handle
                    .join()
                    .map_err(|_| anyhow!("parallel decision worker panicked"))?;
                match result {
                    DecisionWorkerResult::Completed(result) => {
                        results.push(DecisionBatchItem::Completed(result));
                    }
                    DecisionWorkerResult::Skipped(result) => {
                        if result.error.is_transient() {
                            results.push(DecisionBatchItem::Skipped(result));
                        } else {
                            return Err(anyhow!(
                                "decision for agent {} failed: {}",
                                result.agent_id,
                                result.error
                            ));
                        }
                    }
                }
            }
            results.sort_by_key(DecisionBatchItem::agent_id);
            Ok(results)
        })
    }

    fn run_parallel_conversation_turns(
        &self,
        llm: &dyn LlmAdapter,
        turns: Vec<PreparedConversationTurn>,
    ) -> Result<Vec<ConversationBatchItem>> {
        thread::scope(|scope| -> Result<Vec<ConversationBatchItem>> {
            let mut handles = Vec::with_capacity(turns.len());
            for turn in turns {
                handles.push(scope.spawn(move || {
                    let conversation_id = turn.conversation_id;
                    llm.generate_conversation_turn(&turn.input).map_or_else(
                        |error| {
                            ConversationWorkerResult::Interrupted(InterruptedConversationTurn {
                                conversation_id,
                                speaker_id: turn.speaker_id,
                                listener_id: turn.listener_id,
                                error,
                            })
                        },
                        |output| {
                            ConversationWorkerResult::Completed(CompletedConversationTurn {
                                conversation_id,
                                speaker_id: turn.speaker_id,
                                listener_id: turn.listener_id,
                                output,
                            })
                        },
                    )
                }));
            }

            let mut results = Vec::with_capacity(handles.len());
            for handle in handles {
                let result = handle
                    .join()
                    .map_err(|_| anyhow!("parallel conversation worker panicked"))?;
                match result {
                    ConversationWorkerResult::Completed(result) => {
                        results.push(ConversationBatchItem::Completed(result));
                    }
                    ConversationWorkerResult::Interrupted(result) => {
                        if result.error.is_transient() {
                            results.push(ConversationBatchItem::Interrupted(result));
                        } else {
                            return Err(anyhow!(
                                "conversation {} failed: {}",
                                result.conversation_id,
                                result.error
                            ));
                        }
                    }
                }
            }
            results.sort_by_key(ConversationBatchItem::conversation_id);
            Ok(results)
        })
    }

    fn handle_transient_decision_failure(
        &mut self,
        agent_id: u64,
        cognition_trigger: &str,
        social_opportunity_signature: Option<String>,
        error: &LlmError,
    ) -> Result<()> {
        let agent_name = self.agent_name(agent_id)?;
        self.record_cognition_trigger(agent_id, cognition_trigger)?;
        self.record_social_opportunity_signature(agent_id, social_opportunity_signature)?;
        self.set_thought(
            agent_id,
            "Uma falha transitória atrapalhou meu raciocínio neste momento.".to_string(),
        )?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::CognitionFailure,
            summary: format!(
                "{} perde a deliberacao deste tick por falha transitória do provider: {}.",
                agent_name, error
            ),
            impact_tags: vec![
                "llm".to_string(),
                "timeout".to_string(),
                "cognicao".to_string(),
            ],
        });
        Ok(())
    }

    fn handle_transient_conversation_failure(
        &mut self,
        conversation_id: ConversationId,
        speaker_id: u64,
        listener_id: u64,
        error: &LlmError,
    ) -> Result<()> {
        let speaker_name = self.agent_name(speaker_id)?;
        let listener_name = self.agent_name(listener_id)?;
        self.end_conversation(
            conversation_id,
            ConversationStatus::Interrupted,
            ConversationOutcome::ProviderTimeout,
            format!(
                "timeout_llm: {} nao conseguiu responder a {} por falha transitória do provider ({})",
                speaker_name, listener_name, error
            ),
        )
    }

    fn decision_trigger_for_context(
        &mut self,
        context: &AgentContext,
        recent_events: &[WorldEvent],
    ) -> Result<Option<String>> {
        if context.last_intent.is_none() {
            return Ok(Some("sem_intencao".to_string()));
        }

        if context.blocked_ticks >= BLOCKED_RECONSIDERATION_TICKS {
            return Ok(Some("bloqueio_repetido".to_string()));
        }

        if self.has_material_need_shift(context) {
            return Ok(Some("mudanca_material_de_necessidade".to_string()));
        }

        if self.has_direct_social_event(context.id, recent_events) {
            return Ok(Some("evento_social_direto".to_string()));
        }

        if let Some(signature) = self.social_opportunity_signature(context) {
            if context
                .last_social_opportunity_signature
                .as_ref()
                .map(|last| last != &signature)
                .unwrap_or(true)
            {
                return Ok(Some("nova_oportunidade_social".to_string()));
            }
        }

        if context.path_len > 0 && context.current_destination != Some(context.position) {
            return Ok(None);
        }

        if self.total_ticks >= context.next_reconsideration_tick {
            return Ok(Some("horizonte_expirado".to_string()));
        }

        let heartbeat = match context.last_intent.as_ref().map(|intent| intent.kind) {
            Some(IntentKind::Socializar) => SOCIAL_HEARTBEAT_TICKS,
            _ => ROUTINE_HEARTBEAT_TICKS,
        };
        if self.total_ticks >= context.cooldown_until.saturating_add(heartbeat) {
            return Ok(Some("heartbeat_raro".to_string()));
        }

        Ok(None)
    }

    fn context_depth_for_trigger(&self, trigger: &str) -> &'static str {
        match trigger {
            "nova_oportunidade_social"
            | "evento_social_direto"
            | "bloqueio_repetido"
            | "mudanca_material_de_necessidade" => "expanded",
            "horizonte_expirado" | "heartbeat_raro" => "compact",
            _ => "normal",
        }
    }

    fn context_limits_for_trigger(&self, trigger: &str) -> (usize, usize, usize, usize) {
        match self.context_depth_for_trigger(trigger) {
            "compact" => (2, 2, 2, 2),
            "expanded" => (self.relevant_memory_limit, 4, 4, 5),
            _ => (3, 3, 3, 3),
        }
    }

    fn has_material_need_shift(&self, context: &AgentContext) -> bool {
        (context.state.hunger - context.last_deliberation_hunger).abs() >= 15
            || (context.state.energy - context.last_deliberation_energy).abs() >= 15
            || (context.state.health - context.last_deliberation_health).abs() >= 10
            || (context.state.stress - context.last_deliberation_stress).abs() >= 15
    }

    fn has_direct_social_event(&self, agent_id: u64, recent_events: &[WorldEvent]) -> bool {
        recent_events.iter().rev().take(4).any(|event| {
            (event.actor == agent_id || event.target == Some(agent_id))
                && matches!(
                    event.kind,
                    EventKind::Conflict
                        | EventKind::SocialBond
                        | EventKind::ConversationStarted
                        | EventKind::ConversationEnded
                )
        })
    }

    fn social_opportunity_signature(&mut self, context: &AgentContext) -> Option<String> {
        let mut nearby = context
            .relations
            .iter()
            .filter_map(|(other_id, relation)| {
                let distance = self.agent_distance_from(context.position, *other_id)?;
                if distance != 1 {
                    return None;
                }
                if relation.friendship >= 25 || relation.trust >= 25 {
                    return Some(format!("amigo:{other_id}"));
                }
                if relation.resentment >= 20 {
                    return Some(format!("rival:{other_id}"));
                }
                None
            })
            .collect::<Vec<_>>();
        nearby.sort();
        nearby.into_iter().next()
    }

    fn build_psychological_context(
        &self,
        context: &AgentContext,
        recent_events: &[WorldEvent],
        relevant_memories: &[crate::agent_mind::RelevantMemoryInput],
        trigger: &str,
    ) -> PsychologicalContextInput {
        self.build_psychological_context_for_values(
            context.id,
            &context.profile,
            &context.state,
            &context.memories,
            recent_events,
            relevant_memories,
            trigger,
        )
    }

    fn build_psychological_context_for_values(
        &self,
        _agent_id: u64,
        profile: &AgentProfile,
        state: &AgentState,
        memories: &[AgentMemory],
        recent_events: &[WorldEvent],
        relevant_memories: &[crate::agent_mind::RelevantMemoryInput],
        trigger: &str,
    ) -> PsychologicalContextInput {
        PsychologicalContextInput {
            core_values: profile.values.iter().take(3).cloned().collect(),
            long_term_desires: profile.long_term_desires.iter().take(3).cloned().collect(),
            fears: profile.fears.iter().take(3).cloned().collect(),
            social_style: profile.social_style.clone(),
            moral_tolerances: profile.moral_tolerances.iter().take(3).cloned().collect(),
            inner_conflicts: self.derive_inner_conflicts(profile, state, relevant_memories),
            current_identity_tension: self.current_identity_tension(profile, state, trigger),
            dominant_preoccupations: self.dominant_preoccupations(state, recent_events),
            recent_self_narrative: self.recent_self_narrative(memories, recent_events),
        }
    }

    fn build_relational_history(
        &self,
        speaker_id: u64,
        listener_id: u64,
        relation: &AgentRelation,
        speaker_memories: &[AgentMemory],
    ) -> RelationalHistoryInput {
        let mut shared_memories = speaker_memories
            .iter()
            .filter(|memory| memory.about.contains(&listener_id))
            .collect::<Vec<_>>();
        shared_memories.sort_by_key(|memory| {
            -((memory.emotional_weight * 1000) + memory.day as i32 * 10 + memory.tick as i32)
        });

        let shared_history = shared_memories
            .iter()
            .take(5)
            .map(|memory| memory.summary.clone())
            .collect::<Vec<_>>();
        let open_promises = shared_memories
            .iter()
            .filter(|memory| {
                memory.kind == MemoryKind::Promise
                    || memory.tags.iter().any(|tag| tag.contains("prom"))
            })
            .take(3)
            .map(|memory| memory.summary.clone())
            .collect::<Vec<_>>();
        let unresolved_offenses = shared_memories
            .iter()
            .filter(|memory| {
                memory.kind == MemoryKind::Offense
                    || memory.tags.iter().any(|tag| tag.contains("ofens"))
            })
            .take(3)
            .map(|memory| memory.summary.clone())
            .collect::<Vec<_>>();
        let recent_favors = shared_memories
            .iter()
            .filter(|memory| {
                memory.tags.iter().any(|tag| {
                    tag.contains("favor") || tag.contains("ajuda") || tag.contains("divida")
                })
            })
            .take(3)
            .map(|memory| memory.summary.clone())
            .collect::<Vec<_>>();

        RelationalHistoryInput {
            relationship_summary: format!(
                "Entre {} e {}: confianca {}, amizade {}, ressentimento {}, divida moral {}.",
                speaker_id,
                listener_id,
                relation.trust,
                relation.friendship,
                relation.resentment,
                relation.moral_debt
            ),
            shared_history,
            open_promises,
            unresolved_offenses,
            recent_favors,
            trust_trajectory: if relation.trust >= 25 {
                "confianca em alta".to_string()
            } else if relation.trust <= -10 {
                "confianca abalada".to_string()
            } else {
                "confianca oscilante".to_string()
            },
            resentment_trajectory: if relation.resentment >= 25 {
                "ressentimento acumulado".to_string()
            } else if relation.resentment <= 5 {
                "ressentimento baixo".to_string()
            } else {
                "ressentimento latente".to_string()
            },
            social_imbalance: if relation.moral_debt > 10 {
                "o falante sente credito moral".to_string()
            } else if relation.moral_debt < -10 {
                "o falante sente dever ao outro".to_string()
            } else {
                "a relacao parece relativamente equilibrada".to_string()
            },
        }
    }

    fn derive_inner_conflicts(
        &self,
        profile: &AgentProfile,
        state: &AgentState,
        relevant_memories: &[crate::agent_mind::RelevantMemoryInput],
    ) -> Vec<String> {
        let mut conflicts = Vec::new();
        if state.hunger >= 60 && !profile.values.is_empty() {
            conflicts.push(format!(
                "{} disputa espaco com a fome imediata.",
                profile.values[0]
            ));
        }
        if state.stress >= 55 && !profile.fears.is_empty() {
            conflicts.push(format!(
                "O medo de {} pressiona a necessidade de agir.",
                profile.fears[0]
            ));
        }
        if state.energy <= 25 && !state.active_goals.is_empty() {
            conflicts.push(format!(
                "O corpo pede pausa, mas {} continua urgente.",
                state.active_goals[0]
            ));
        }
        if conflicts.is_empty() && !relevant_memories.is_empty() {
            conflicts.push(format!(
                "As lembrancas de {} continuam pesando.",
                relevant_memories[0].summary
            ));
        }
        conflicts.truncate(3);
        conflicts
    }

    fn current_identity_tension(
        &self,
        profile: &AgentProfile,
        state: &AgentState,
        trigger: &str,
    ) -> String {
        if trigger.contains("social") && !profile.values.is_empty() && !profile.fears.is_empty() {
            return format!(
                "{} tenta proteger {} sem ativar {}.",
                state.current_focus, profile.values[0], profile.fears[0]
            );
        }
        if state.stress >= 60 {
            return format!(
                "{} luta para manter o autocontrole sob stress.",
                state.current_focus
            );
        }
        format!(
            "{} tenta alinhar rotina, reputacao e desejo de longo prazo.",
            state.current_focus
        )
    }

    fn dominant_preoccupations(
        &self,
        state: &AgentState,
        recent_events: &[WorldEvent],
    ) -> Vec<String> {
        let mut concerns = Vec::new();
        concerns.extend(state.active_goals.iter().take(2).cloned());
        if state.hunger >= 60 {
            concerns.push("fome crescente".to_string());
        }
        if state.energy <= 30 {
            concerns.push("fadiga".to_string());
        }
        if state.stress >= 55 {
            concerns.push("stress alto".to_string());
        }
        concerns.extend(
            recent_events
                .iter()
                .take(2)
                .map(|event| event.summary.clone()),
        );
        concerns.truncate(4);
        concerns
    }

    fn recent_self_narrative(
        &self,
        memories: &[AgentMemory],
        recent_events: &[WorldEvent],
    ) -> String {
        let mut pieces = memories
            .iter()
            .rev()
            .take(2)
            .map(|memory| memory.summary.clone())
            .collect::<Vec<_>>();
        pieces.extend(
            recent_events
                .iter()
                .take(2)
                .map(|event| event.summary.clone()),
        );
        if pieces.is_empty() {
            "Nada recente reorganizou a mente do agente.".to_string()
        } else {
            pieces.join(" | ")
        }
    }

    fn build_economic_context(&self, context: &AgentContext) -> EconomicContextInput {
        let household = context
            .household_id
            .and_then(|household_id| self.household_by_id(household_id));
        let pantry = household
            .map(|household| {
                household
                    .pantry
                    .iter()
                    .map(|stack| format!("{} x{}", stack.kind.as_str(), stack.amount))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let pending_salary = household
            .map(|household| household.pending_payments.iter().map(|claim| claim.amount).sum())
            .unwrap_or(0);
        let tax_pressure = household
            .map(|household| self.village_economy.daily_household_tax + household.tax_arrears)
            .unwrap_or(0);
        let work_obligations = self.work_obligations_for_context(context);
        let local_prices = self
            .local_prices_for_agent(context.position)
            .into_iter()
            .map(|price| format!("{}={} moedas", price.resource.as_str(), price.unit_price))
            .collect::<Vec<_>>();
        let base_resource_availability = [
            ResourceKind::Graos,
            ResourceKind::Lenha,
            ResourceKind::MetalBruto,
        ]
        .into_iter()
        .map(|resource| {
            let total: i32 = self
                .establishments
                .iter()
                .map(|establishment| Self::total_resource_amount(&establishment.stock, resource))
                .sum();
            format!("{} disponivel localmente: {}", resource.as_str(), total)
        })
        .collect::<Vec<_>>();
        let scarcity_signals = self
            .village_economy
            .scarcity_metrics
            .iter()
            .filter(|metric| metric.pressure > 0)
            .take(4)
            .map(|metric| format!("escassez de {} ({})", metric.resource.as_str(), metric.pressure))
            .collect::<Vec<_>>();
        let public_treasury_status = if self.village_economy.public_treasury < 12 {
            format!(
                "caixa publico baixo ({}) e risco de atraso civico",
                self.village_economy.public_treasury
            )
        } else {
            format!("caixa publico estavel ({})", self.village_economy.public_treasury)
        };
        let open_tasks = context
            .household_id
            .map(|household_id| self.open_tasks_for_household(household_id))
            .unwrap_or_default();

        EconomicContextInput {
            household_name: household
                .map(|household| household.name.clone())
                .unwrap_or_else(|| "Sem lar".to_string()),
            household_treasury: household.map(|household| household.treasury).unwrap_or(0),
            pantry,
            pending_salary,
            tax_pressure,
            work_obligations,
            local_prices,
            base_resource_availability,
            scarcity_signals,
            public_treasury_status,
            open_tasks,
        }
    }

    fn work_obligations_for_context(&self, context: &AgentContext) -> Vec<String> {
        let mut obligations = Vec::new();
        if context.role == Role::Farmer {
            for establishment in self.establishments.iter().filter(|establishment| {
                matches!(
                    establishment.kind,
                    LocationKind::Farm | LocationKind::Woodlot | LocationKind::Quarry
                )
            }) {
                for target in &establishment.stock_targets {
                    let current = Self::total_resource_amount(&establishment.stock, target.kind);
                    if current < target.amount {
                        obligations.push(format!(
                            "{} abaixo do alvo em {}",
                            target.kind.as_str(),
                            establishment.name
                        ));
                    }
                }
            }
        } else if let Some(building_id) = self.work_building_id_for_role(context.role)
            && let Some(establishment) = self.establishment_by_building(building_id)
        {
            for target in &establishment.stock_targets {
                let current = Self::total_resource_amount(&establishment.stock, target.kind);
                if current < target.amount {
                    obligations.push(format!(
                        "{} abaixo do alvo em {}",
                        target.kind.as_str(),
                        establishment.name
                    ));
                }
            }
        }
        obligations.truncate(4);
        obligations
    }

    fn work_building_id_for_role(&self, role: Role) -> Option<BuildingId> {
        self.establishments
            .iter()
            .find(|establishment| match role {
                Role::Farmer => establishment.kind == LocationKind::Farm,
                Role::Blacksmith => establishment.kind == LocationKind::Workshop,
                Role::Baker => establishment.kind == LocationKind::Bakery,
                Role::TavernKeeper => establishment.kind == LocationKind::Tavern,
                Role::Guard => establishment.kind == LocationKind::GuardPost,
                Role::Headman => establishment.kind == LocationKind::Manor,
            })
            .and_then(|establishment| establishment.building_id)
    }

    fn open_tasks_for_household(&self, household_id: BuildingId) -> Vec<EconomicOpportunityInput> {
        let mut tasks = self
            .economic_tasks
            .iter()
            .filter(|task| {
                task.actor_household_id == household_id
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
            })
            .map(|task| EconomicOpportunityInput {
                kind: task.kind,
                summary: task.description.clone(),
                resource: task.resource,
                amount: task.amount,
                unit_price: (task.unit_price > 0).then_some(task.unit_price),
            })
            .collect::<Vec<_>>();
        tasks.truncate(6);
        tasks
    }

    fn normalized_reconsideration_horizon(&self, kind: IntentKind) -> u64 {
        match kind {
            IntentKind::Socializar => 1,
            IntentKind::Comer => 2,
            IntentKind::Descansar => 3,
            IntentKind::Refletir => 3,
            IntentKind::Andar => 2,
            IntentKind::Comprar => 4,
            IntentKind::Transportar => 4,
            IntentKind::Vender => 4,
            IntentKind::ReceberPagamento => 3,
            IntentKind::Trabalhar => ROUTINE_RECONSIDERATION_MAX as u64,
        }
    }

    fn increment_blocked_ticks(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<CognitionComponent>()
            .ok_or_else(|| anyhow!("missing cognition component"))?
            .blocked_ticks += 1;
        Ok(())
    }

    fn reset_blocked_ticks(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<CognitionComponent>()
            .ok_or_else(|| anyhow!("missing cognition component"))?
            .blocked_ticks = 0;
        Ok(())
    }

    fn record_cognition_trigger(&mut self, agent_id: u64, trigger: &str) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<CognitionComponent>()
            .ok_or_else(|| anyhow!("missing cognition component"))?
            .last_cognition_trigger = Some(trigger.to_string());
        Ok(())
    }

    fn record_social_opportunity_signature(
        &mut self,
        agent_id: u64,
        signature: Option<String>,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<CognitionComponent>()
            .ok_or_else(|| anyhow!("missing cognition component"))?
            .last_social_opportunity_signature = signature;
        Ok(())
    }

    fn apply_conversation_turn_output(
        &mut self,
        conversation_id: ConversationId,
        speaker_id: u64,
        listener_id: u64,
        output: crate::agent_mind::ConversationTurnOutput,
    ) -> Result<()> {
        let speaker_name = self.agent_name(speaker_id)?;
        let listener_name = self.agent_name(listener_id)?;
        let turn = ConversationTurn {
            speaker_id,
            listener_id,
            tick: self.total_ticks,
            utterance: output.utterance.clone(),
            speech_act: output.speech_act.clone(),
            emotion: output.emotion.clone(),
            tone: output.tone.clone(),
        };

        self.apply_relation_delta(speaker_id, listener_id, &output.relation_delta_hint)?;
        self.apply_relation_delta(
            listener_id,
            speaker_id,
            &invert_delta(&output.relation_delta_hint),
        )?;
        self.apply_conversation_effects(
            speaker_id,
            listener_id,
            &output.speech_act,
            &output.emotion,
            output.risk_shift.unwrap_or(0),
            &output.belief_updates,
        )?;

        let (should_end, end_status, end_outcome, end_reason) = {
            let conversation = self
                .conversation_state_mut(conversation_id)
                .ok_or_else(|| anyhow!("conversation {conversation_id} not found"))?;
            conversation.turn_count += 1;
            conversation.summary = extend_summary(
                &conversation.summary,
                &format!("{speaker_name}: {}", output.utterance),
            );
            conversation.recent_turns.push(turn);
            if conversation.recent_turns.len() > CONVERSATION_RECENT_TURNS_LIMIT {
                let overflow = conversation.recent_turns.len() - CONVERSATION_RECENT_TURNS_LIMIT;
                conversation.recent_turns.drain(0..overflow);
            }
            if let Some(participant) = conversation
                .participant_states
                .iter_mut()
                .find(|participant| participant.agent_id == speaker_id)
            {
                participant.last_speech_act = Some(output.speech_act.clone());
                participant.last_emotion = Some(output.emotion.clone());
                if let Some(goal) = output.belief_updates.first() {
                    participant.social_goal = goal.clone();
                }
            }

            let should_end = if !output.intent_to_continue {
                Some((
                    ConversationStatus::Ended,
                    ConversationOutcome::OneSidedExit,
                    format!("{speaker_name} decide encerrar a conversa."),
                ))
            } else if conversation.turn_count >= conversation.max_turns {
                Some((
                    ConversationStatus::Ended,
                    ConversationOutcome::MaxTurns,
                    "a conversa atingiu o limite de turnos".to_string(),
                ))
            } else {
                conversation.current_speaker_id = listener_id;
                None
            };
            (
                should_end.is_some(),
                should_end
                    .as_ref()
                    .map(|tuple| tuple.0.clone())
                    .unwrap_or(ConversationStatus::Active),
                should_end
                    .as_ref()
                    .map(|tuple| tuple.1.clone())
                    .unwrap_or(ConversationOutcome::Ongoing),
                should_end.map(|tuple| tuple.2).unwrap_or_default(),
            )
        };

        self.set_last_social_act(speaker_id, output.speech_act.clone())?;
        self.set_last_social_act(listener_id, format!("ouve {}", output.speech_act))?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: speaker_id,
            target: Some(listener_id),
            kind: EventKind::ConversationTurn,
            summary: format!(
                "{speaker_name} fala com {listener_name}: {}",
                output.utterance
            ),
            impact_tags: vec![
                "social".to_string(),
                "conversa".to_string(),
                output.speech_act.clone(),
            ],
        });

        if should_end {
            self.end_conversation(conversation_id, end_status, end_outcome, end_reason)?;
        }
        Ok(())
    }

    fn apply_conversation_effects(
        &mut self,
        speaker_id: u64,
        listener_id: u64,
        speech_act: &str,
        emotion: &str,
        risk_shift: i32,
        belief_updates: &[String],
    ) -> Result<()> {
        let speaker_name = self.agent_name(speaker_id)?;
        let listener_name = self.agent_name(listener_id)?;
        for agent_id in [speaker_id, listener_id] {
            let entity = self.find_agent_entity(agent_id)?;
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.stress = (state.0.stress + risk_shift).clamp(0, 100);
            state.0.energy = (state.0.energy - 1).clamp(0, 100);
            if let Some(goal) = belief_updates.first() {
                if !state.0.active_goals.iter().any(|existing| existing == goal) {
                    state.0.active_goals.push(goal.clone());
                }
                if state.0.active_goals.len() > 4 {
                    state.0.active_goals.truncate(4);
                }
            }
        }
        self.set_thought(
            speaker_id,
            format!("Quero {} {}.", speech_act, listener_name),
        )?;
        self.set_thought(
            listener_id,
            format!("{speaker_name} fala comigo com emocao {emotion}."),
        )?;
        self.add_memory(
            speaker_id,
            MemoryKind::Reflection,
            format!("Eu disse a {}: {}", listener_name, speech_act),
            vec!["social".to_string(), "conversa".to_string()],
            6,
            vec![listener_id],
        )?;
        self.add_memory(
            listener_id,
            MemoryKind::Impression,
            format!("{speaker_name} falou comigo: {speech_act}"),
            vec!["social".to_string(), "conversa".to_string()],
            6,
            vec![speaker_id],
        )?;
        Ok(())
    }

    fn end_conversation(
        &mut self,
        conversation_id: ConversationId,
        status: ConversationStatus,
        outcome: ConversationOutcome,
        reason: String,
    ) -> Result<()> {
        let (participants, summary) = {
            let conversation = self
                .conversation_state_mut(conversation_id)
                .ok_or_else(|| anyhow!("conversation {conversation_id} not found"))?;
            conversation.status = status.clone();
            conversation.outcome = outcome.clone();
            conversation.end_reason = Some(reason.clone());
            (conversation.participants, conversation.summary.clone())
        };

        let [agent_a, agent_b] = participants;
        for (agent_id, other_id) in [(agent_a, agent_b), (agent_b, agent_a)] {
            let other_name = self.agent_name(other_id)?;
            self.release_agent_from_conversation(agent_id, reason.clone())?;
            self.add_memory(
                agent_id,
                if matches!(outcome, ConversationOutcome::PhysicalConflict) {
                    MemoryKind::Offense
                } else {
                    MemoryKind::Impression
                },
                format!("Conversa com {} terminou: {}", other_name, summary),
                vec!["social".to_string(), "conversa".to_string()],
                14,
                vec![other_id],
            )?;
        }
        let agent_a_name = self.agent_name(agent_a)?;
        let agent_b_name = self.agent_name(agent_b)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_a,
            target: Some(agent_b),
            kind: EventKind::ConversationEnded,
            summary: format!(
                "Conversa entre {} e {} termina: {}.",
                agent_a_name, agent_b_name, reason
            ),
            impact_tags: vec![
                "social".to_string(),
                "conversa".to_string(),
                format!("outcome:{:?}", outcome).to_lowercase(),
            ],
        });
        Ok(())
    }

    fn push_event(&mut self, event: WorldEvent) {
        self.events.push(event);
        if self.events.len() > 5_000 {
            let overflow = self.events.len() - 5_000;
            self.events.drain(0..overflow);
        }
    }

    fn add_memory(
        &mut self,
        agent_id: u64,
        kind: MemoryKind,
        summary: String,
        tags: Vec<String>,
        weight: i32,
        about: Vec<u64>,
    ) -> Result<()> {
        let memory = AgentMemory {
            id: self.next_memory_id,
            day: self.day,
            tick: self.tick_of_day,
            kind,
            summary: summary.clone(),
            details: summary,
            emotional_weight: weight,
            about,
            tags,
        };
        self.next_memory_id += 1;
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut memories = entity_mut
            .get_mut::<MemoryComponent>()
            .ok_or_else(|| anyhow!("missing memory component"))?;
        memories.0.push(memory);
        if memories.0.len() > 64 {
            let overflow = memories.0.len() - 64;
            memories.0.drain(0..overflow);
        }
        Ok(())
    }

    fn find_agent_entity(&mut self, agent_id: u64) -> Result<Entity> {
        let mut query = self.world.query::<(Entity, &AgentCore)>();
        query
            .iter(&self.world)
            .find_map(|(entity, core)| (core.id == agent_id).then_some(entity))
            .ok_or_else(|| anyhow!("agent {agent_id} not found"))
    }

    fn agent_name(&mut self, agent_id: u64) -> Result<String> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .name
            .clone())
    }

    fn agent_initial(&mut self, agent_id: u64) -> Option<char> {
        self.agent_name(agent_id)
            .ok()
            .and_then(|name| name.chars().next())
            .map(|ch| ch.to_ascii_uppercase())
    }

    fn agent_state(&mut self, agent_id: u64) -> Result<AgentState> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<StateComponent>()
            .ok_or_else(|| anyhow!("missing state component"))?
            .0
            .clone())
    }

    fn agent_profile(&mut self, agent_id: u64) -> Result<AgentProfile> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<ProfileComponent>()
            .ok_or_else(|| anyhow!("missing profile component"))?
            .0
            .clone())
    }

    fn agent_memories(&mut self, agent_id: u64) -> Result<Vec<AgentMemory>> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<MemoryComponent>()
            .ok_or_else(|| anyhow!("missing memory component"))?
            .0
            .clone())
    }

    fn relation_between(&mut self, agent_id: u64, other_id: u64) -> AgentRelation {
        let Ok(entity) = self.find_agent_entity(agent_id) else {
            return AgentRelation::default();
        };
        self.world
            .entity(entity)
            .get::<RelationComponent>()
            .and_then(|relations| relations.0.get(&other_id))
            .cloned()
            .unwrap_or_default()
    }

    fn apply_relation_delta(
        &mut self,
        agent_id: u64,
        other_id: u64,
        delta: &RelationDelta,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut relations = entity_mut
            .get_mut::<RelationComponent>()
            .ok_or_else(|| anyhow!("missing relation component"))?;
        let relation = relations.0.entry(other_id).or_default();
        relation.trust = (relation.trust + delta.trust).clamp(-100, 100);
        relation.friendship = (relation.friendship + delta.friendship).clamp(-100, 100);
        relation.resentment = (relation.resentment + delta.resentment).clamp(-100, 100);
        relation.attraction = (relation.attraction + delta.attraction).clamp(-100, 100);
        relation.moral_debt = (relation.moral_debt + delta.moral_debt).clamp(-100, 100);
        relation.reputation = (relation.reputation + delta.reputation).clamp(-100, 100);
        relation.last_updated_day = self.day;
        Ok(())
    }

    fn occupancy_map(&mut self) -> HashMap<TileCoord, u64> {
        let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
        query
            .iter(&self.world)
            .map(|(core, position)| (position.0, core.id))
            .collect()
    }

    fn agent_distance_from(&mut self, origin: TileCoord, other_id: u64) -> Option<i32> {
        let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
        query.iter(&self.world).find_map(|(core, position)| {
            (core.id == other_id).then_some(origin.manhattan(position.0))
        })
    }

    fn is_occupied(&mut self, coord: TileCoord, ignore_agent_id: Option<u64>) -> bool {
        let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
        query
            .iter(&self.world)
            .any(|(core, position)| position.0 == coord && Some(core.id) != ignore_agent_id)
    }

    fn tile_at(&self, coord: TileCoord) -> Option<&TileSpec> {
        if coord.x < 0
            || coord.y < 0
            || coord.x >= self.spatial.grid.width
            || coord.y >= self.spatial.grid.height
        {
            return None;
        }
        let index = (coord.y * self.spatial.grid.width + coord.x) as usize;
        self.spatial.grid.tiles.get(index)
    }

    fn is_walkable(&self, coord: TileCoord) -> bool {
        let Some(tile) = self.tile_at(coord) else {
            return false;
        };
        if !tile.kind.walkable() {
            return false;
        }
        !self
            .spatial
            .fixtures
            .iter()
            .any(|fixture| fixture.coord == coord && fixture.blocks_movement)
    }

    fn fixture_at(&self, coord: TileCoord) -> Option<&FixtureSpec> {
        self.spatial
            .fixtures
            .iter()
            .find(|fixture| fixture.coord == coord)
    }

    fn building_name(&self, building_id: BuildingId) -> Option<String> {
        self.spatial
            .buildings
            .iter()
            .find(|building| building.id == building_id)
            .map(|building| building.name.clone())
    }

    fn building_kind(&self, building_id: BuildingId) -> Option<LocationKind> {
        self.spatial
            .buildings
            .iter()
            .find(|building| building.id == building_id)
            .map(|building| building.kind)
    }

    fn building_kind_opt(&self, building_id: Option<BuildingId>) -> Option<LocationKind> {
        building_id.and_then(|id| self.building_kind(id))
    }

    fn room_name(&self, room_id: RoomId) -> Option<String> {
        self.spatial
            .rooms
            .iter()
            .find(|room| room.id == room_id)
            .map(|room| room.name.clone())
    }

    fn area_name(&self, coord: TileCoord) -> String {
        if let Some(tile) = self.tile_at(coord) {
            if let Some(building_id) = tile.building_id {
                return self
                    .building_name(building_id)
                    .unwrap_or_else(|| "Interior".to_string());
            }
            return match tile.kind {
                TileKind::Field => "Campos do Leste".to_string(),
                TileKind::Forest => "Bosque de Coleta".to_string(),
                TileKind::Rock => "Pedreira do Norte".to_string(),
                TileKind::Road => {
                    if (20..=28).contains(&coord.x) && (11..=15).contains(&coord.y) {
                        "Praca Central".to_string()
                    } else {
                        "Estrada da Vila".to_string()
                    }
                }
                _ => "Exterior da Vila".to_string(),
            };
        }
        "Fora do Mundo".to_string()
    }

    fn accessible_exits(&self, coord: TileCoord) -> Vec<String> {
        let mut exits = Vec::new();
        if let Some(tile) = self.tile_at(coord) {
            if let Some(building_id) = tile.building_id {
                if let Some(building) = self
                    .spatial
                    .buildings
                    .iter()
                    .find(|building| building.id == building_id)
                {
                    exits.push(format!(
                        "porta para exterior em ({}, {})",
                        building.entrance.x, building.entrance.y
                    ));
                }
            } else {
                for building in self.spatial.buildings.iter() {
                    if coord.manhattan(building.entrance) <= 8 {
                        exits.push(format!("entrada de {}", building.name));
                    }
                }
            }
        }
        exits
    }

    fn local_blockers(&self, coord: TileCoord) -> Vec<String> {
        let mut blockers = Vec::new();
        for neighbor in coord.neighbors4() {
            if let Some(tile) = self.tile_at(neighbor) {
                if tile.kind == TileKind::Wall {
                    blockers.push("parede".to_string());
                }
            }
        }
        blockers
    }

    fn nearby_fixture_inputs(&self, coord: TileCoord, radius: i32) -> Vec<NearbyFixtureInput> {
        let mut fixtures = self
            .spatial
            .fixtures
            .iter()
            .filter_map(|fixture| {
                let distance = coord.manhattan(fixture.coord);
                (distance <= radius).then(|| NearbyFixtureInput {
                    id: fixture.id,
                    name: fixture.name.clone(),
                    kind: fixture.kind,
                    distance,
                    building_name: fixture.building_id.and_then(|id| self.building_name(id)),
                    room_name: fixture.room_id.and_then(|id| self.room_name(id)),
                })
            })
            .collect::<Vec<_>>();
        fixtures.sort_by_key(|fixture| fixture.distance);
        fixtures
    }

    fn nearby_agent_inputs(
        &mut self,
        agent_id: u64,
        coord: TileCoord,
        current_room_id: Option<RoomId>,
        relations: &HashMap<u64, AgentRelation>,
    ) -> Vec<NearbyAgentInput> {
        let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
        let mut agents = query
            .iter(&self.world)
            .filter(|(core, _)| core.id != agent_id)
            .filter_map(|(core, position)| {
                let distance = coord.manhattan(position.0);
                (distance <= 6).then(|| NearbyAgentInput {
                    id: core.id,
                    name: core.name.clone(),
                    role: core.role.as_str().to_string(),
                    distance,
                    same_room: self.tile_at(position.0).and_then(|tile| tile.room_id)
                        == current_room_id,
                    relation: relations.get(&core.id).cloned(),
                })
            })
            .collect::<Vec<_>>();
        agents.sort_by_key(|agent| agent.distance);
        agents
    }

    fn recent_events_for(&self, agent_id: u64, coord: TileCoord, limit: usize) -> Vec<WorldEvent> {
        self.events
            .iter()
            .rev()
            .filter(|event| {
                event.actor == agent_id
                    || event.target == Some(agent_id)
                    || event
                        .impact_tags
                        .iter()
                        .any(|tag| tag == &self.area_name(coord))
            })
            .take(limit)
            .cloned()
            .collect()
    }

    fn tile_tags(&self, coord: TileCoord) -> Vec<String> {
        let mut tags = vec![self.area_name(coord)];
        if let Some(tile) = self.tile_at(coord) {
            if let Some(building_id) = tile.building_id {
                tags.push(format!("building:{building_id}"));
            }
            if let Some(room_id) = tile.room_id {
                tags.push(format!("room:{room_id}"));
            }
        }
        tags
    }

    fn nearest_storage_for_building(&self, building_id: Option<BuildingId>) -> Option<FixtureId> {
        self.spatial
            .fixtures
            .iter()
            .find(|fixture| {
                fixture.building_id == building_id && fixture.kind == FixtureKind::Storage
            })
            .map(|fixture| fixture.id)
    }

    fn consume_food_for_agent(&mut self, agent_id: u64) -> Result<bool> {
        if let Some(household_id) = self.household_id_for_agent(agent_id)
            && let Some(household) = self.household_by_id_mut(household_id)
        {
            if consume_matching(
                &mut household.pantry,
                &[ResourceKind::Pao, ResourceKind::Caldo, ResourceKind::Graos],
            ) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn fixture_access_tile(&self, fixture: &FixtureSpec) -> Option<TileCoord> {
        self.access_tile_for_coord(fixture.coord)
    }

    fn access_tile_for_coord(&self, coord: TileCoord) -> Option<TileCoord> {
        coord
            .neighbors4()
            .into_iter()
            .find(|neighbor| self.is_walkable(*neighbor))
    }

    fn find_path(
        &mut self,
        start: TileCoord,
        goal: TileCoord,
        ignore_agent_id: Option<u64>,
    ) -> Option<Vec<TileCoord>> {
        if start == goal {
            return Some(Vec::new());
        }
        let mut frontier = VecDeque::new();
        let mut came_from: HashMap<TileCoord, TileCoord> = HashMap::new();
        let mut visited: HashSet<TileCoord> = HashSet::new();
        frontier.push_back(start);
        visited.insert(start);

        while let Some(current) = frontier.pop_front() {
            for neighbor in current.neighbors4() {
                if !visited.contains(&neighbor)
                    && self.is_walkable(neighbor)
                    && !self.is_occupied(neighbor, ignore_agent_id)
                {
                    visited.insert(neighbor);
                    came_from.insert(neighbor, current);
                    if neighbor == goal {
                        return Some(reconstruct_path(start, goal, &came_from));
                    }
                    frontier.push_back(neighbor);
                }
            }
        }
        None
    }

    fn agents_adjacent(&mut self, actor_id: u64, target_id: u64) -> Result<bool> {
        let actor = self.debug_agent_position(actor_id)?;
        let target = self.debug_agent_position(target_id)?;
        Ok(actor.manhattan(target) == 1)
    }

    fn conversation_state(&self, conversation_id: ConversationId) -> Option<ConversationState> {
        self.conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .cloned()
    }

    fn conversation_state_mut(
        &mut self,
        conversation_id: ConversationId,
    ) -> Option<&mut ConversationState> {
        self.conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
    }

    fn agent_conversation_id(&mut self, agent_id: u64) -> Result<Option<ConversationId>> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?
            .active_conversation_id)
    }

    fn agent_social_cooldown_until(&mut self, agent_id: u64) -> Result<u64> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?
            .social_cooldown_until)
    }

    fn bind_agent_to_conversation(
        &mut self,
        agent_id: u64,
        conversation_id: ConversationId,
        partner_id: u64,
        social_act: String,
    ) -> Result<()> {
        self.clear_intent_navigation(agent_id)?;
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut conversation = entity_mut
            .get_mut::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?;
        conversation.active_conversation_id = Some(conversation_id);
        conversation.conversation_partner_id = Some(partner_id);
        conversation.last_social_act = Some(social_act);
        Ok(())
    }

    fn release_agent_from_conversation(&mut self, agent_id: u64, social_act: String) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut conversation = entity_mut
            .get_mut::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?;
        conversation.active_conversation_id = None;
        conversation.conversation_partner_id = None;
        conversation.last_social_act = Some(social_act);
        conversation.social_cooldown_until = self.total_ticks + 2;
        Ok(())
    }

    fn set_last_social_act(&mut self, agent_id: u64, social_act: String) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?
            .last_social_act = Some(social_act);
        Ok(())
    }

    fn set_thought(&mut self, agent_id: u64, thought: String) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<ThoughtComponent>()
            .ok_or_else(|| anyhow!("missing thought component"))?
            .0 = thought;
        Ok(())
    }

    fn agent_name_map(&mut self) -> HashMap<u64, String> {
        let mut query = self.world.query::<&AgentCore>();
        query
            .iter(&self.world)
            .map(|core| (core.id, core.name.clone()))
            .collect()
    }

    fn household_by_id(&self, household_id: BuildingId) -> Option<&HouseholdEconomy> {
        self.households
            .iter()
            .find(|household| household.id == household_id)
    }

    fn household_by_id_mut(&mut self, household_id: BuildingId) -> Option<&mut HouseholdEconomy> {
        self.households
            .iter_mut()
            .find(|household| household.id == household_id)
    }

    fn household_id_for_agent(&mut self, agent_id: u64) -> Option<BuildingId> {
        let entity = self.find_agent_entity(agent_id).ok()?;
        self.world.entity(entity).get::<AgentCore>()?.home_building_id
    }

    fn establishment_by_id(&self, establishment_id: EstablishmentId) -> Option<&EstablishmentEconomy> {
        self.establishments
            .iter()
            .find(|establishment| establishment.id == establishment_id)
    }

    fn establishment_by_id_mut(
        &mut self,
        establishment_id: EstablishmentId,
    ) -> Option<&mut EstablishmentEconomy> {
        self.establishments
            .iter_mut()
            .find(|establishment| establishment.id == establishment_id)
    }

    fn establishment_by_building(&self, building_id: BuildingId) -> Option<&EstablishmentEconomy> {
        self.establishments
            .iter()
            .find(|establishment| establishment.building_id == Some(building_id))
    }

    fn establishment_by_building_mut(
        &mut self,
        building_id: BuildingId,
    ) -> Option<&mut EstablishmentEconomy> {
        self.establishments
            .iter_mut()
            .find(|establishment| establishment.building_id == Some(building_id))
    }

    fn building_by_id(&self, building_id: BuildingId) -> Option<&BuildingSpec> {
        self.spatial
            .buildings
            .iter()
            .find(|building| building.id == building_id)
    }

    fn fixture_by_id(&self, fixture_id: FixtureId) -> Option<&FixtureSpec> {
        self.spatial
            .fixtures
            .iter()
            .find(|fixture| fixture.id == fixture_id)
    }

    fn economic_task_summary(&self, task_id: EconomicTaskId) -> Option<String> {
        self.economic_tasks
            .iter()
            .find(|task| task.id == task_id && task.phase != EconomicTaskPhase::Completed)
            .map(|task| task.description.clone())
    }

    fn local_prices_for_agent(&self, position: TileCoord) -> Vec<PostedPrice> {
        let mut prices = self
            .establishments
            .iter()
            .filter(|establishment| {
                establishment
                    .building_id
                    .and_then(|building_id| self.building_by_id(building_id))
                    .map(|building| building.entrance.manhattan(position) <= 20)
                    .unwrap_or(false)
            })
            .flat_map(|establishment| establishment.posted_prices.clone())
            .collect::<Vec<_>>();
        prices.sort_by_key(|price| (price.resource.as_str().to_string(), price.unit_price));
        prices.truncate(8);
        prices
    }

    fn total_resource_amount(stacks: &[ResourceStack], kind: ResourceKind) -> i32 {
        stacks
            .iter()
            .filter(|stack| stack.kind == kind)
            .map(|stack| stack.amount.max(0))
            .sum()
    }

    fn total_food_units(stacks: &[ResourceStack]) -> i32 {
        stacks
            .iter()
            .filter(|stack| stack.kind.is_food())
            .map(|stack| stack.amount.max(0))
            .sum()
    }

    fn take_resource(stacks: &mut Vec<ResourceStack>, kind: ResourceKind, amount: i32) -> i32 {
        if amount <= 0 {
            return 0;
        }
        let mut remaining = amount;
        let mut taken = 0;
        for stack in stacks.iter_mut().filter(|stack| stack.kind == kind) {
            if remaining <= 0 {
                break;
            }
            let delta = stack.amount.min(remaining);
            if delta > 0 {
                stack.amount -= delta;
                remaining -= delta;
                taken += delta;
            }
        }
        stacks.retain(|stack| stack.amount > 0);
        taken
    }

    fn push_resource(stacks: &mut Vec<ResourceStack>, kind: ResourceKind, amount: i32) {
        if amount > 0 {
            merge_stack(stacks, ResourceStack { kind, amount });
        }
    }

    fn base_price(&self, resource: ResourceKind) -> i32 {
        self.village_economy
            .base_prices
            .iter()
            .find(|price| price.resource == resource)
            .map(|price| price.unit_price)
            .unwrap_or(1)
    }

    fn sync_establishment_stocks_to_fixtures(&mut self) {
        let updates = self
            .establishments
            .iter()
            .filter_map(|establishment| {
                establishment
                    .storage_fixture_id
                    .map(|fixture_id| (fixture_id, establishment.stock.clone()))
            })
            .collect::<Vec<_>>();
        for (fixture_id, stock) in updates {
            if let Some(fixture) = self
                .spatial
                .fixtures
                .iter_mut()
                .find(|fixture| fixture.id == fixture_id)
            {
                fixture.stock = stock;
            }
        }
    }

    fn sync_household_pantries_to_fixtures(&mut self) {
        let updates = self
            .households
            .iter()
            .filter_map(|household| {
                self.nearest_storage_for_building(Some(household.id))
                    .map(|fixture_id| (fixture_id, household.pantry.clone()))
            })
            .collect::<Vec<_>>();
        for (fixture_id, stock) in updates {
            if let Some(fixture) = self
                .spatial
                .fixtures
                .iter_mut()
                .find(|fixture| fixture.id == fixture_id)
            {
                fixture.stock = stock;
            }
        }
    }

    fn refresh_economy_state(&mut self) -> Result<()> {
        for household in &mut self.households {
            let food_units = Self::total_food_units(&household.pantry);
            household.scarcity_pressure = (household.minimum_food_units - food_units).max(0);
        }

        let recalculated = self
            .establishments
            .iter()
            .map(|establishment| {
                let posted_prices = self.recalculate_posted_prices(establishment);
                (establishment.id, posted_prices)
            })
            .collect::<Vec<_>>();
        for (establishment_id, posted_prices) in recalculated {
            if let Some(establishment) = self.establishment_by_id_mut(establishment_id) {
                establishment.posted_prices = posted_prices;
            }
        }

        self.village_economy.scarcity_metrics = self.compute_scarcity_metrics();
        self.ensure_economic_tasks();
        self.sync_establishment_stocks_to_fixtures();
        self.sync_household_pantries_to_fixtures();
        Ok(())
    }

    fn close_daily_economy(&mut self) -> Result<()> {
        let daily_tax = self.village_economy.daily_household_tax;
        let current_day = self.day;
        let tax_results = self
            .households
            .iter()
            .map(|household| {
                let owed = daily_tax + household.tax_arrears;
                let paid = household.treasury.min(owed.max(0));
                let arrears = owed - paid;
                (
                    household.id,
                    household.name.clone(),
                    household.member_ids.first().copied().unwrap_or(0),
                    owed,
                    paid,
                    arrears,
                )
            })
            .collect::<Vec<_>>();
        for (household_id, household_name, actor_id, owed, paid, arrears) in tax_results {
            if let Some(household) = self.household_by_id_mut(household_id) {
                household.treasury -= paid;
                household.tax_arrears = arrears.max(0);
                if paid > 0 {
                    household.last_tax_paid_day = current_day;
                }
            }
            self.village_economy.public_treasury += paid;
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: actor_id,
                target: None,
                kind: EventKind::Tax,
                summary: if paid >= owed {
                    format!("{household_name} paga {paid} moeda(s) de imposto ao caixa publico.")
                } else if paid > 0 {
                    format!(
                        "{household_name} paga apenas {paid}/{owed} moeda(s) de imposto; fica devendo {}.",
                        arrears.max(0)
                    )
                } else {
                    format!(
                        "{household_name} nao consegue pagar imposto; debito acumulado em {} moeda(s).",
                        arrears.max(0)
                    )
                },
                impact_tags: vec!["imposto".to_string(), "caixa_publico".to_string()],
            });
        }

        let distributions = self
            .establishments
            .iter()
            .filter(|establishment| {
                !establishment.public_service && !establishment.owner_household_ids.is_empty()
            })
            .map(|establishment| {
                let reserve = 30;
                let distributable = (establishment.cash - reserve).max(0);
                (
                    establishment.id,
                    establishment.owner_household_ids.clone(),
                    distributable,
                )
            })
            .filter(|(_, owners, distributable)| !owners.is_empty() && *distributable > 0)
            .collect::<Vec<_>>();

        for (establishment_id, owners, distributable) in distributions {
            let share = distributable / owners.len() as i32;
            if share <= 0 {
                continue;
            }
            if let Some(establishment) = self.establishment_by_id_mut(establishment_id) {
                establishment.cash -= share * owners.len() as i32;
            }
            for owner in owners {
                if let Some(household) = self.household_by_id_mut(owner) {
                    household.treasury += share;
                }
            }
        }
        self.sync_household_pantries_to_fixtures();
        self.sync_establishment_stocks_to_fixtures();
        Ok(())
    }

    fn recalculate_posted_prices(&self, establishment: &EstablishmentEconomy) -> Vec<PostedPrice> {
        establishment
            .stock_targets
            .iter()
            .map(|target| {
                let current = Self::total_resource_amount(&establishment.stock, target.kind);
                let shortage = (target.amount - current).max(0);
                let mut unit_price = self.base_price(target.kind) + shortage / 2;
                if establishment.cash < 10 {
                    unit_price += 1;
                }
                if let Some(quote) = self
                    .village_economy
                    .external_quotes
                    .iter()
                    .find(|quote| quote.resource == target.kind)
                {
                    unit_price = unit_price.clamp(quote.sell_price.max(1), quote.buy_price.max(1));
                }
                PostedPrice {
                    resource: target.kind,
                    unit_price: unit_price.max(1),
                }
            })
            .collect()
    }

    fn compute_scarcity_metrics(&self) -> Vec<ScarcityMetric> {
        let mut metrics = Vec::new();
        for resource in [
            ResourceKind::Graos,
            ResourceKind::Lenha,
            ResourceKind::MetalBruto,
            ResourceKind::Pao,
            ResourceKind::Caldo,
            ResourceKind::Ferramentas,
        ] {
            let available: i32 = self
                .establishments
                .iter()
                .map(|establishment| Self::total_resource_amount(&establishment.stock, resource))
                .sum::<i32>()
                + self
                    .households
                    .iter()
                    .map(|household| Self::total_resource_amount(&household.pantry, resource))
                    .sum::<i32>();
            let target: i32 = self
                .establishments
                .iter()
                .map(|establishment| {
                    establishment
                        .stock_targets
                        .iter()
                        .find(|target| target.kind == resource)
                        .map(|target| target.amount)
                        .unwrap_or(0)
                })
                .sum();
            metrics.push(ScarcityMetric {
                resource,
                pressure: (target - available).max(0),
            });
        }
        metrics
    }

    fn ensure_economic_tasks(&mut self) {
        self.economic_tasks
            .retain(|task| task.phase != EconomicTaskPhase::Completed && task.phase != EconomicTaskPhase::Failed);
        self.ensure_local_production_tasks();
        self.ensure_household_food_tasks();
        self.ensure_establishment_supply_tasks();
        self.ensure_payment_tasks();
        self.ensure_surplus_sale_tasks();
    }

    fn has_open_task_for(
        &self,
        household_id: BuildingId,
        kind: EconomicTaskKind,
        resource: Option<ResourceKind>,
        destination: &EconomicNode,
    ) -> bool {
        self.economic_tasks.iter().any(|task| {
            task.actor_household_id == household_id
                && task.kind == kind
                && task.resource == resource
                && task.phase != EconomicTaskPhase::Completed
                && task.phase != EconomicTaskPhase::Failed
                && &task.destination == destination
        })
    }

    fn next_task_id(&mut self) -> EconomicTaskId {
        let task_id = self.next_economic_task_id;
        self.next_economic_task_id += 1;
        task_id
    }

    fn ensure_household_food_tasks(&mut self) {
        let households = self.households.clone();
        for household in households {
            let food_units = Self::total_food_units(&household.pantry);
            if food_units >= household.minimum_food_units {
                continue;
            }
            let Some((establishment_id, resource, unit_price)) =
                self.best_food_source_for_household(household.id)
            else {
                continue;
            };
            let destination = EconomicNode::HouseholdPantry(household.id);
            if self.has_open_task_for(
                household.id,
                EconomicTaskKind::Comprar,
                Some(resource),
                &destination,
            ) {
                continue;
            }
            let amount = (household.minimum_food_units - food_units).clamp(1, 3);
            let task_id = self.next_task_id();
            self.economic_tasks.push(EconomicTask {
                id: task_id,
                kind: EconomicTaskKind::Comprar,
                actor_household_id: household.id,
                assigned_agent_id: None,
                source: EconomicNode::Establishment(establishment_id),
                destination,
                resource: Some(resource),
                amount,
                unit_price,
                total_price: unit_price * amount,
                description: format!("Comprar {} x{} para {}", resource.as_str(), amount, household.name),
                phase: EconomicTaskPhase::AwaitingPickup,
                related_establishment_id: Some(establishment_id),
            });
        }
    }

    fn ensure_local_production_tasks(&mut self) {
        let establishments = self.establishments.clone();
        for establishment in establishments {
            let Some(resource) = self.primary_output_for_kind(establishment.kind) else {
                continue;
            };
            let target = establishment
                .stock_targets
                .iter()
                .find(|target| target.kind == resource)
                .map(|target| target.amount)
                .unwrap_or(0);
            let current = Self::total_resource_amount(&establishment.stock, resource);
            if current >= target {
                continue;
            }
            let Some(actor_household_id) = establishment.owner_household_ids.first().copied() else {
                continue;
            };
            let destination = EconomicNode::Establishment(establishment.id);
            if self.has_open_task_for(
                actor_household_id,
                EconomicTaskKind::Produzir,
                Some(resource),
                &destination,
            ) {
                continue;
            }
            let amount = match resource {
                ResourceKind::Graos => 4,
                ResourceKind::Lenha => 3,
                ResourceKind::MetalBruto => 2,
                _ => 1,
            };
            let task_id = self.next_task_id();
            self.economic_tasks.push(EconomicTask {
                id: task_id,
                kind: EconomicTaskKind::Produzir,
                actor_household_id,
                assigned_agent_id: None,
                source: destination.clone(),
                destination,
                resource: Some(resource),
                amount,
                unit_price: 0,
                total_price: 0,
                description: match resource {
                    ResourceKind::Graos => format!("Produzir graos em {}", establishment.name),
                    ResourceKind::Lenha => format!("Coletar lenha em {}", establishment.name),
                    ResourceKind::MetalBruto => {
                        format!("Extrair metal bruto em {}", establishment.name)
                    }
                    ResourceKind::Ferramentas => {
                        format!("Forjar ferramentas em {}", establishment.name)
                    }
                    ResourceKind::Pao => format!("Assar pao em {}", establishment.name),
                    ResourceKind::Caldo => format!("Preparar caldo em {}", establishment.name),
                    ResourceKind::Moedas => format!("Trabalhar em {}", establishment.name),
                },
                phase: EconomicTaskPhase::AwaitingPickup,
                related_establishment_id: Some(establishment.id),
            });
        }
    }

    fn ensure_establishment_supply_tasks(&mut self) {
        let establishments = self.establishments.clone();
        for establishment in establishments {
            match establishment.kind {
                LocationKind::Bakery | LocationKind::Tavern => {
                    self.ensure_transfer_shortage_task(
                        &establishment,
                        ResourceKind::Graos,
                        LocationKind::Farm,
                        3,
                    );
                    if !self.ensure_transfer_shortage_task(
                        &establishment,
                        ResourceKind::Lenha,
                        LocationKind::Woodlot,
                        2,
                    ) {
                        self.ensure_external_purchase_task(&establishment, ResourceKind::Lenha, 2);
                    }
                }
                LocationKind::Workshop => {
                    if !self.ensure_transfer_shortage_task(
                        &establishment,
                        ResourceKind::MetalBruto,
                        LocationKind::Quarry,
                        2,
                    ) {
                        self.ensure_external_purchase_task(
                            &establishment,
                            ResourceKind::MetalBruto,
                            2,
                        );
                    }
                    if !self.ensure_transfer_shortage_task(
                        &establishment,
                        ResourceKind::Lenha,
                        LocationKind::Woodlot,
                        2,
                    ) {
                        self.ensure_external_purchase_task(&establishment, ResourceKind::Lenha, 2);
                    }
                }
                LocationKind::Farm => {
                    self.ensure_transfer_shortage_task(
                        &establishment,
                        ResourceKind::Ferramentas,
                        LocationKind::Workshop,
                        1,
                    );
                }
                LocationKind::Woodlot | LocationKind::Quarry => {}
                _ => {}
            }
        }
    }

    fn ensure_payment_tasks(&mut self) {
        let households = self.households.clone();
        for household in households {
            if household.pending_payments.is_empty() {
                continue;
            }
            let destination = EconomicNode::HouseholdPantry(household.id);
            if self.has_open_task_for(
                household.id,
                EconomicTaskKind::ReceberPagamento,
                Some(ResourceKind::Moedas),
                &destination,
            ) {
                continue;
            }
            let total_amount: i32 = household.pending_payments.iter().map(|claim| claim.amount).sum();
            let task_id = self.next_task_id();
            self.economic_tasks.push(EconomicTask {
                id: task_id,
                kind: EconomicTaskKind::ReceberPagamento,
                actor_household_id: household.id,
                assigned_agent_id: None,
                source: EconomicNode::PublicTreasury,
                destination,
                resource: Some(ResourceKind::Moedas),
                amount: total_amount.max(1),
                unit_price: 1,
                total_price: total_amount.max(1),
                description: format!("Receber pagamentos pendentes para {}", household.name),
                phase: EconomicTaskPhase::AwaitingPickup,
                related_establishment_id: None,
            });
        }
    }

    fn ensure_surplus_sale_tasks(&mut self) {
        let establishments = self.establishments.clone();
        for establishment in establishments {
            let Some(primary_output) = self.primary_output_for_kind(establishment.kind) else {
                continue;
            };
            let target = establishment
                .stock_targets
                .iter()
                .find(|target| target.kind == primary_output)
                .map(|target| target.amount)
                .unwrap_or(0);
            let current = Self::total_resource_amount(&establishment.stock, primary_output);
            if current <= target + 3 {
                continue;
            }
            if self.has_open_task_for(
                establishment
                    .owner_household_ids
                    .first()
                    .copied()
                    .unwrap_or_default(),
                EconomicTaskKind::Vender,
                Some(primary_output),
                &EconomicNode::ExternalMarket,
            ) {
                continue;
            }
            if let Some(actor_household_id) = establishment.owner_household_ids.first().copied() {
                let unit_price = self
                    .village_economy
                    .external_quotes
                    .iter()
                    .find(|quote| quote.resource == primary_output)
                    .map(|quote| quote.sell_price)
                    .unwrap_or(self.base_price(primary_output));
                let task_id = self.next_task_id();
                self.economic_tasks.push(EconomicTask {
                    id: task_id,
                    kind: EconomicTaskKind::Vender,
                    actor_household_id,
                    assigned_agent_id: None,
                    source: EconomicNode::Establishment(establishment.id),
                    destination: EconomicNode::ExternalMarket,
                    resource: Some(primary_output),
                    amount: 2,
                    unit_price,
                    total_price: unit_price * 2,
                    description: format!("Vender excedente de {} em {}", primary_output.as_str(), establishment.name),
                    phase: EconomicTaskPhase::AwaitingPickup,
                    related_establishment_id: Some(establishment.id),
                });
            }
        }
    }

    fn best_food_source_for_household(
        &self,
        household_id: BuildingId,
    ) -> Option<(EstablishmentId, ResourceKind, i32)> {
        let mut offers = self
            .establishments
            .iter()
            .filter_map(|establishment| {
                let best_stock = [ResourceKind::Caldo, ResourceKind::Pao, ResourceKind::Graos]
                    .into_iter()
                    .find(|resource| Self::total_resource_amount(&establishment.stock, *resource) > 0)?;
                let unit_price = establishment
                    .posted_prices
                    .iter()
                    .find(|price| price.resource == best_stock)
                    .map(|price| price.unit_price)
                    .unwrap_or(self.base_price(best_stock));
                Some((establishment.id, best_stock, unit_price))
            })
            .collect::<Vec<_>>();
        offers.sort_by_key(|(_, resource, price)| (*price, resource.as_str().to_string()));
        let treasury = self
            .household_by_id(household_id)
            .map(|household| household.treasury)
            .unwrap_or(0);
        offers.into_iter().find(|(_, _, price)| treasury >= *price)
    }

    fn ensure_external_purchase_task(
        &mut self,
        establishment: &EstablishmentEconomy,
        resource: ResourceKind,
        amount: i32,
    ) {
        let current = Self::total_resource_amount(&establishment.stock, resource);
        let target = establishment
            .stock_targets
            .iter()
            .find(|target| target.kind == resource)
            .map(|target| target.amount)
            .unwrap_or(0);
        let Some(actor_household_id) = establishment.owner_household_ids.first().copied() else {
            return;
        };
        if current >= target {
            return;
        }
        let destination = EconomicNode::Establishment(establishment.id);
        if self.has_open_task_for(
            actor_household_id,
            EconomicTaskKind::Comprar,
            Some(resource),
            &destination,
        ) {
            return;
        }
        let unit_price = self
            .village_economy
            .external_quotes
            .iter()
            .find(|quote| quote.resource == resource)
            .map(|quote| quote.buy_price)
            .unwrap_or(self.base_price(resource));
        let task_id = self.next_task_id();
        self.economic_tasks.push(EconomicTask {
            id: task_id,
            kind: EconomicTaskKind::Comprar,
            actor_household_id,
            assigned_agent_id: None,
            source: EconomicNode::ExternalMarket,
            destination,
            resource: Some(resource),
            amount,
            unit_price,
            total_price: unit_price * amount,
            description: format!("Comprar {} x{} para {}", resource.as_str(), amount, establishment.name),
            phase: EconomicTaskPhase::AwaitingPickup,
            related_establishment_id: Some(establishment.id),
        });
    }

    fn ensure_transfer_shortage_task(
        &mut self,
        destination_establishment: &EstablishmentEconomy,
        resource: ResourceKind,
        source_kind: LocationKind,
        amount: i32,
    ) -> bool {
        let current = Self::total_resource_amount(&destination_establishment.stock, resource);
        let target = destination_establishment
            .stock_targets
            .iter()
            .find(|target| target.kind == resource)
            .map(|target| target.amount)
            .unwrap_or(0);
        if current >= target {
            return true;
        }
        let Some(source) = self
            .establishments
            .iter()
            .find(|candidate| {
                candidate.kind == source_kind
                    && Self::total_resource_amount(&candidate.stock, resource) >= amount
            })
            .cloned()
        else {
            return false;
        };
        let Some(actor_household_id) = destination_establishment.owner_household_ids.first().copied() else {
            return false;
        };
        let destination = EconomicNode::Establishment(destination_establishment.id);
        if self.has_open_task_for(
            actor_household_id,
            EconomicTaskKind::Transportar,
            Some(resource),
            &destination,
        ) {
            return true;
        }
        let task_id = self.next_task_id();
        self.economic_tasks.push(EconomicTask {
            id: task_id,
            kind: EconomicTaskKind::Transportar,
            actor_household_id,
            assigned_agent_id: None,
            source: EconomicNode::Establishment(source.id),
            destination,
            resource: Some(resource),
            amount,
            unit_price: 0,
            total_price: 0,
            description: format!(
                "Transportar {} x{} de {} para {}",
                resource.as_str(),
                amount,
                source.name,
                destination_establishment.name
            ),
            phase: EconomicTaskPhase::AwaitingPickup,
            related_establishment_id: Some(destination_establishment.id),
        });
        true
    }

    fn primary_output_for_kind(&self, kind: LocationKind) -> Option<ResourceKind> {
        match kind {
            LocationKind::Farm => Some(ResourceKind::Graos),
            LocationKind::Woodlot => Some(ResourceKind::Lenha),
            LocationKind::Quarry => Some(ResourceKind::MetalBruto),
            LocationKind::Workshop => Some(ResourceKind::Ferramentas),
            LocationKind::Bakery => Some(ResourceKind::Pao),
            LocationKind::Tavern => Some(ResourceKind::Caldo),
            _ => None,
        }
    }

    fn conversation_map(&self) -> HashMap<ConversationId, ConversationState> {
        self.conversations
            .iter()
            .map(|conversation| (conversation.id, conversation.clone()))
            .collect()
    }

    fn active_conversation_participants(&self) -> HashSet<u64> {
        self.conversations
            .iter()
            .filter(|conversation| conversation.status == ConversationStatus::Active)
            .flat_map(|conversation| conversation.participants)
            .collect()
    }

    fn agent_path(&mut self, agent_id: u64) -> Option<Vec<TileCoord>> {
        let entity = self.find_agent_entity(agent_id).ok()?;
        self.world
            .entity(entity)
            .get::<PathComponent>()
            .map(|path| path.0.clone())
    }
}

#[derive(Clone)]
struct AgentContext {
    id: u64,
    name: String,
    role: Role,
    position: TileCoord,
    state: AgentState,
    profile: AgentProfile,
    relations: HashMap<u64, AgentRelation>,
    memories: Vec<AgentMemory>,
    current_destination: Option<TileCoord>,
    path_len: usize,
    destination_label: Option<String>,
    current_building_id: Option<BuildingId>,
    current_room_id: Option<RoomId>,
    last_intent: Option<AgentIntent>,
    cooldown_until: u64,
    llm_calls: u64,
    next_reconsideration_tick: u64,
    blocked_ticks: u32,
    last_social_opportunity_signature: Option<String>,
    last_deliberation_hunger: i32,
    last_deliberation_energy: i32,
    last_deliberation_health: i32,
    last_deliberation_stress: i32,
    active_conversation_id: Option<ConversationId>,
    social_cooldown_until: u64,
    household_id: Option<BuildingId>,
}

struct PreparedDecisionRequest {
    agent_id: u64,
    nearby_ids: Vec<u64>,
    cognition_trigger: String,
    social_opportunity_signature: Option<String>,
    input: DecisionInput,
}

struct CompletedDecisionRequest {
    agent_id: u64,
    nearby_ids: Vec<u64>,
    cognition_trigger: String,
    social_opportunity_signature: Option<String>,
    envelope: DecisionEnvelope,
}

struct SkippedDecisionRequest {
    agent_id: u64,
    cognition_trigger: String,
    social_opportunity_signature: Option<String>,
    error: LlmError,
}

enum DecisionWorkerResult {
    Completed(CompletedDecisionRequest),
    Skipped(SkippedDecisionRequest),
}

enum DecisionBatchItem {
    Completed(CompletedDecisionRequest),
    Skipped(SkippedDecisionRequest),
}

impl DecisionBatchItem {
    fn agent_id(&self) -> u64 {
        match self {
            Self::Completed(result) => result.agent_id,
            Self::Skipped(result) => result.agent_id,
        }
    }
}

struct PreparedConversationTurn {
    conversation_id: ConversationId,
    speaker_id: u64,
    listener_id: u64,
    input: ConversationTurnInput,
}

struct CompletedConversationTurn {
    conversation_id: ConversationId,
    speaker_id: u64,
    listener_id: u64,
    output: ConversationTurnOutput,
}

struct InterruptedConversationTurn {
    conversation_id: ConversationId,
    speaker_id: u64,
    listener_id: u64,
    error: LlmError,
}

enum ConversationWorkerResult {
    Completed(CompletedConversationTurn),
    Interrupted(InterruptedConversationTurn),
}

enum ConversationBatchItem {
    Completed(CompletedConversationTurn),
    Interrupted(InterruptedConversationTurn),
}

impl ConversationBatchItem {
    fn conversation_id(&self) -> ConversationId {
        match self {
            Self::Completed(result) => result.conversation_id,
            Self::Interrupted(result) => result.conversation_id,
        }
    }
}

impl AgentContext {
    fn profile_summary(&self) -> Vec<String> {
        let mut summary = self.profile.values.clone();
        summary.extend(self.profile.long_term_desires.clone());
        summary.extend(self.profile.fears.clone());
        summary
    }
}

#[derive(Clone)]
struct ResolvedTargetCandidate {
    destination: TileCoord,
    label: String,
}

#[derive(Clone)]
struct SeedAgentTemplate {
    id: u64,
    name: String,
    role: Role,
    profile: AgentProfile,
    state: AgentState,
    relations: HashMap<u64, AgentRelation>,
    memories: Vec<AgentMemory>,
    inventory: Vec<ResourceStack>,
    last_thought: String,
}

#[derive(Clone, Copy)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl Rect {
    fn border_tiles(self) -> Vec<TileCoord> {
        let mut tiles = Vec::new();
        for y in self.y1..=self.y2 {
            for x in self.x1..=self.x2 {
                if x == self.x1 || x == self.x2 || y == self.y1 || y == self.y2 {
                    tiles.push(TileCoord { x, y });
                }
            }
        }
        tiles
    }

    fn interior_tiles(self) -> Vec<TileCoord> {
        let mut tiles = Vec::new();
        for y in (self.y1 + 1)..self.y2 {
            for x in (self.x1 + 1)..self.x2 {
                tiles.push(TileCoord { x, y });
            }
        }
        tiles
    }

    fn footprint(self) -> Vec<TileCoord> {
        let mut tiles = Vec::new();
        for y in self.y1..=self.y2 {
            for x in self.x1..=self.x2 {
                tiles.push(TileCoord { x, y });
            }
        }
        tiles
    }
}

#[derive(Clone)]
struct FixturePlacement {
    kind: FixtureKind,
    coord: TileCoord,
    name: &'static str,
    blocks_movement: bool,
    stock: Vec<ResourceStack>,
}

fn generate_village(width: i32, height: i32, _seed: u64) -> SpatialSnapshot {
    let mut builder = SpatialBuilder::new(width, height);
    builder.fill(TileKind::Grass);

    builder.carve_road_rect(Rect {
        x1: 20,
        y1: 11,
        x2: 28,
        y2: 15,
    });
    builder.carve_road_line(TileCoord { x: 2, y: 13 }, TileCoord { x: 45, y: 13 });
    builder.carve_road_line(TileCoord { x: 24, y: 2 }, TileCoord { x: 24, y: 25 });
    builder.carve_road_line(TileCoord { x: 5, y: 7 }, TileCoord { x: 5, y: 13 });
    builder.carve_road_line(TileCoord { x: 13, y: 7 }, TileCoord { x: 13, y: 13 });
    builder.carve_road_line(TileCoord { x: 32, y: 5 }, TileCoord { x: 24, y: 5 });
    builder.carve_road_line(TileCoord { x: 14, y: 19 }, TileCoord { x: 14, y: 22 });
    builder.carve_road_line(TileCoord { x: 34, y: 19 }, TileCoord { x: 34, y: 22 });
    builder.carve_road_line(TileCoord { x: 24, y: 22 }, TileCoord { x: 45, y: 22 });
    builder.carve_road_line(TileCoord { x: 4, y: 13 }, TileCoord { x: 4, y: 17 });
    builder.carve_road_line(TileCoord { x: 41, y: 5 }, TileCoord { x: 32, y: 5 });

    builder.carve_field_rect(Rect {
        x1: 36,
        y1: 24,
        x2: 46,
        y2: 26,
    });
    builder.carve_terrain_rect(
        Rect {
            x1: 1,
            y1: 23,
            x2: 8,
            y2: 26,
        },
        TileKind::Forest,
    );
    builder.carve_terrain_rect(
        Rect {
            x1: 40,
            y1: 8,
            x2: 46,
            y2: 11,
        },
        TileKind::Rock,
    );
    builder.carve_field_rect(Rect {
        x1: 1,
        y1: 20,
        x2: 1,
        y2: 20,
    });

    builder.add_building(
        "Casa da Rua Alta I",
        LocationKind::Home,
        Rect {
            x1: 2,
            y1: 2,
            x2: 8,
            y2: 7,
        },
        TileCoord { x: 5, y: 7 },
        "Sala Comum",
        "casa",
        vec![
            fixture(FixtureKind::Bed, 3, 3, "Cama 1", true, vec![]),
            fixture(FixtureKind::Bed, 5, 3, "Cama 2", true, vec![]),
            fixture(FixtureKind::Bed, 7, 3, "Cama 3", true, vec![]),
            fixture(FixtureKind::Table, 5, 5, "Mesa da Casa", true, vec![]),
            fixture(
                FixtureKind::Storage,
                7,
                5,
                "Armario da Casa",
                true,
                vec![ResourceStack {
                    kind: ResourceKind::Pao,
                    amount: 3,
                }],
            ),
        ],
    );
    builder.add_building(
        "Casa da Rua Alta II",
        LocationKind::Home,
        Rect {
            x1: 10,
            y1: 2,
            x2: 16,
            y2: 7,
        },
        TileCoord { x: 13, y: 7 },
        "Sala Comum",
        "casa",
        vec![
            fixture(FixtureKind::Bed, 11, 3, "Cama 4", true, vec![]),
            fixture(FixtureKind::Bed, 13, 3, "Cama 5", true, vec![]),
            fixture(FixtureKind::Bed, 15, 3, "Cama 6", true, vec![]),
            fixture(FixtureKind::Table, 13, 5, "Mesa da Casa", true, vec![]),
            fixture(
                FixtureKind::Storage,
                15,
                5,
                "Armario da Casa",
                true,
                vec![ResourceStack {
                    kind: ResourceKind::Pao,
                    amount: 3,
                }],
            ),
        ],
    );
    builder.add_building(
        "Solar do Conselho",
        LocationKind::Manor,
        Rect {
            x1: 19,
            y1: 2,
            x2: 29,
            y2: 8,
        },
        TileCoord { x: 24, y: 8 },
        "Sala do Conselho",
        "manor",
        vec![
            fixture(FixtureKind::Table, 24, 4, "Mesa do Conselho", true, vec![]),
            fixture(
                FixtureKind::Seat,
                22,
                4,
                "Assento do Conselho",
                true,
                vec![],
            ),
            fixture(
                FixtureKind::Workstation,
                26,
                4,
                "Escrivaninha do Lider",
                true,
                vec![],
            ),
            fixture(
                FixtureKind::Storage,
                27,
                6,
                "Arquivo do Solar",
                true,
                vec![ResourceStack {
                    kind: ResourceKind::Moedas,
                    amount: 0,
                }],
            ),
            fixture(FixtureKind::Bed, 21, 6, "Leito do Lider", true, vec![]),
        ],
    );
    builder.add_building(
        "Posto da Muralha",
        LocationKind::GuardPost,
        Rect {
            x1: 32,
            y1: 2,
            x2: 38,
            y2: 7,
        },
        TileCoord { x: 32, y: 5 },
        "Sala da Guarda",
        "guarda",
        vec![
            fixture(
                FixtureKind::Workstation,
                34,
                4,
                "Mesa da Ronda",
                true,
                vec![],
            ),
            fixture(
                FixtureKind::Storage,
                36,
                4,
                "Arca da Guarda",
                true,
                vec![ResourceStack {
                    kind: ResourceKind::Moedas,
                    amount: 0,
                }],
            ),
            fixture(FixtureKind::Bed, 34, 6, "Catre da Guarda", true, vec![]),
            fixture(FixtureKind::Table, 36, 6, "Mesa da Guarda", true, vec![]),
        ],
    );
    builder.add_building(
        "Forja de Aco Curto",
        LocationKind::Workshop,
        Rect {
            x1: 3,
            y1: 10,
            x2: 11,
            y2: 16,
        },
        TileCoord { x: 11, y: 13 },
        "Sala da Forja",
        "forja",
        vec![
            fixture(FixtureKind::Workstation, 5, 12, "Bigorna", true, vec![]),
            fixture(
                FixtureKind::Storage,
                9,
                12,
                "Baú de Ferramentas",
                true,
                vec![ResourceStack {
                    kind: ResourceKind::Ferramentas,
                    amount: 2,
                }],
            ),
            fixture(FixtureKind::Table, 7, 14, "Mesa da Forja", true, vec![]),
        ],
    );
    builder.add_building(
        "Padaria do Sino",
        LocationKind::Bakery,
        Rect {
            x1: 36,
            y1: 10,
            x2: 44,
            y2: 16,
        },
        TileCoord { x: 36, y: 13 },
        "Sala do Forno",
        "padaria",
        vec![
            fixture(FixtureKind::Workstation, 38, 12, "Forno", true, vec![]),
            fixture(
                FixtureKind::Storage,
                42,
                12,
                "Despensa",
                true,
                vec![
                    ResourceStack {
                        kind: ResourceKind::Pao,
                        amount: 6,
                    },
                    ResourceStack {
                        kind: ResourceKind::Graos,
                        amount: 4,
                    },
                ],
            ),
            fixture(FixtureKind::Table, 40, 14, "Mesa da Padaria", true, vec![]),
        ],
    );
    builder.add_building(
        "Taverna da Chuva",
        LocationKind::Tavern,
        Rect {
            x1: 19,
            y1: 19,
            x2: 29,
            y2: 25,
        },
        TileCoord { x: 24, y: 19 },
        "Sala da Taverna",
        "taverna",
        vec![
            fixture(
                FixtureKind::Workstation,
                22,
                21,
                "Balcao da Taverna",
                true,
                vec![],
            ),
            fixture(
                FixtureKind::Storage,
                27,
                21,
                "Barril da Taverna",
                true,
                vec![
                    ResourceStack {
                        kind: ResourceKind::Caldo,
                        amount: 8,
                    },
                    ResourceStack {
                        kind: ResourceKind::Pao,
                        amount: 4,
                    },
                ],
            ),
            fixture(FixtureKind::Table, 24, 23, "Mesa Longa", true, vec![]),
            fixture(FixtureKind::Seat, 26, 23, "Banco da Taverna", true, vec![]),
        ],
    );
    builder.add_building(
        "Casa da Rua Baixa I",
        LocationKind::Home,
        Rect {
            x1: 11,
            y1: 19,
            x2: 17,
            y2: 24,
        },
        TileCoord { x: 14, y: 19 },
        "Sala Comum",
        "casa",
        vec![
            fixture(FixtureKind::Bed, 12, 20, "Cama 7", true, vec![]),
            fixture(FixtureKind::Bed, 14, 20, "Cama 8", true, vec![]),
            fixture(FixtureKind::Bed, 16, 20, "Cama 9", true, vec![]),
            fixture(FixtureKind::Table, 14, 22, "Mesa da Casa", true, vec![]),
            fixture(
                FixtureKind::Storage,
                16,
                22,
                "Armario da Casa",
                true,
                vec![ResourceStack {
                    kind: ResourceKind::Pao,
                    amount: 3,
                }],
            ),
        ],
    );
    let woodlot_building = builder.add_building(
        "Galpao do Lenhal",
        LocationKind::Woodlot,
        Rect {
            x1: 1,
            y1: 17,
            x2: 7,
            y2: 22,
        },
        TileCoord { x: 4, y: 17 },
        "Abrigo do Lenhal",
        "lenhal",
        vec![
            fixture(
                FixtureKind::Storage,
                5,
                19,
                "Pilha de Lenha",
                true,
                vec![ResourceStack {
                    kind: ResourceKind::Lenha,
                    amount: 8,
                }],
            ),
            fixture(FixtureKind::Table, 3, 19, "Mesa do Lenhal", true, vec![]),
        ],
    );
    builder.add_building(
        "Casa da Rua Baixa II",
        LocationKind::Home,
        Rect {
            x1: 31,
            y1: 19,
            x2: 37,
            y2: 24,
        },
        TileCoord { x: 34, y: 19 },
        "Sala Comum",
        "casa",
        vec![
            fixture(FixtureKind::Bed, 32, 20, "Cama 10", true, vec![]),
            fixture(FixtureKind::Bed, 34, 20, "Cama 11", true, vec![]),
            fixture(FixtureKind::Bed, 36, 20, "Cama 12", true, vec![]),
            fixture(FixtureKind::Table, 34, 22, "Mesa da Casa", true, vec![]),
            fixture(
                FixtureKind::Storage,
                36,
                22,
                "Armario da Casa",
                true,
                vec![ResourceStack {
                    kind: ResourceKind::Pao,
                    amount: 3,
                }],
            ),
        ],
    );
    let quarry_building = builder.add_building(
        "Barracao da Pedreira",
        LocationKind::Quarry,
        Rect {
            x1: 40,
            y1: 2,
            x2: 46,
            y2: 7,
        },
        TileCoord { x: 40, y: 5 },
        "Abrigo da Pedreira",
        "pedreira",
        vec![
            fixture(
                FixtureKind::Storage,
                44,
                4,
                "Caixote de Minerio",
                true,
                vec![ResourceStack {
                    kind: ResourceKind::MetalBruto,
                    amount: 6,
                }],
            ),
            fixture(FixtureKind::Table, 42, 4, "Mesa da Pedreira", true, vec![]),
        ],
    );
    let farm_building = builder.add_building(
        "Celeiro do Leste",
        LocationKind::Farm,
        Rect {
            x1: 39,
            y1: 19,
            x2: 45,
            y2: 24,
        },
        TileCoord { x: 39, y: 22 },
        "Sala do Celeiro",
        "campo",
        vec![
            fixture(
                FixtureKind::Storage,
                43,
                21,
                "Armazem do Celeiro",
                true,
                vec![ResourceStack {
                    kind: ResourceKind::Graos,
                    amount: 10,
                }],
            ),
            fixture(FixtureKind::Table, 41, 21, "Mesa do Celeiro", true, vec![]),
        ],
    );

    builder.add_outdoor_fixture(
        Some(farm_building),
        None,
        FixtureKind::Workstation,
        TileCoord { x: 41, y: 25 },
        "Leira de Plantio",
        false,
        vec![],
    );
    builder.add_outdoor_fixture(
        Some(farm_building),
        None,
        FixtureKind::Workstation,
        TileCoord { x: 44, y: 25 },
        "Sulco de Plantio",
        false,
        vec![],
    );
    builder.add_outdoor_fixture(
        Some(woodlot_building),
        None,
        FixtureKind::Workstation,
        TileCoord { x: 3, y: 24 },
        "Tronco de Corte",
        false,
        vec![],
    );
    builder.add_outdoor_fixture(
        Some(woodlot_building),
        None,
        FixtureKind::Workstation,
        TileCoord { x: 6, y: 25 },
        "Clareira de Coleta",
        false,
        vec![],
    );
    builder.add_outdoor_fixture(
        Some(quarry_building),
        None,
        FixtureKind::Workstation,
        TileCoord { x: 42, y: 9 },
        "Face da Pedreira",
        false,
        vec![],
    );
    builder.add_outdoor_fixture(
        Some(quarry_building),
        None,
        FixtureKind::Workstation,
        TileCoord { x: 45, y: 10 },
        "Veio Exposto",
        false,
        vec![],
    );

    builder.finish()
}

fn work_building_map(spatial: &SpatialSnapshot) -> HashMap<Role, BuildingId> {
    let mut map = HashMap::new();
    for building in &spatial.buildings {
        match building.kind {
            LocationKind::Farm => {
                map.insert(Role::Farmer, building.id);
            }
            LocationKind::Workshop => {
                map.insert(Role::Blacksmith, building.id);
            }
            LocationKind::Bakery => {
                map.insert(Role::Baker, building.id);
            }
            LocationKind::Tavern => {
                map.insert(Role::TavernKeeper, building.id);
            }
            LocationKind::GuardPost => {
                map.insert(Role::Guard, building.id);
            }
            LocationKind::Manor => {
                map.insert(Role::Headman, building.id);
            }
            _ => {}
        }
    }
    map
}

fn home_bed_assignments(spatial: &SpatialSnapshot) -> Vec<(BuildingId, TileCoord)> {
    let mut beds = spatial
        .fixtures
        .iter()
        .filter(|fixture| fixture.kind == FixtureKind::Bed)
        .filter_map(|fixture| {
            fixture
                .building_id
                .map(|building_id| (building_id, fixture.coord))
        })
        .collect::<Vec<_>>();
    beds.sort_by_key(|(_, coord)| (coord.y, coord.x));
    beds
}

fn initialize_economy_state(
    world: &mut World,
    spatial: &SpatialSnapshot,
) -> (Vec<HouseholdEconomy>, Vec<EstablishmentEconomy>, VillageEconomy) {
    let mut home_members = HashMap::<BuildingId, Vec<u64>>::new();
    let mut role_households = HashMap::<Role, Vec<BuildingId>>::new();
    let mut query = world.query::<&AgentCore>();
    for core in query.iter(world) {
        if let Some(home_building_id) = core.home_building_id {
            home_members
                .entry(home_building_id)
                .or_default()
                .push(core.id);
            role_households
                .entry(core.role)
                .or_default()
                .push(home_building_id);
        }
    }
    for households in role_households.values_mut() {
        households.sort_unstable();
        households.dedup();
    }

    let households = spatial
        .buildings
        .iter()
        .filter(|building| building.kind == LocationKind::Home)
        .map(|building| {
            let pantry = spatial
                .fixtures
                .iter()
                .find(|fixture| {
                    fixture.kind == FixtureKind::Storage && fixture.building_id == Some(building.id)
                })
                .map(|fixture| fixture.stock.clone())
                .unwrap_or_default();
            let member_ids = home_members.remove(&building.id).unwrap_or_default();
            HouseholdEconomy {
                id: building.id,
                name: building.name.clone(),
                member_ids: member_ids.clone(),
                treasury: 18,
                pantry,
                minimum_food_units: (member_ids.len() as i32).max(1) * 2,
                pending_payments: Vec::new(),
                scarcity_pressure: 0,
                tax_arrears: 0,
                last_tax_paid_day: 0,
            }
        })
        .collect::<Vec<_>>();

    let base_prices = vec![
        PostedPrice {
            resource: ResourceKind::Graos,
            unit_price: 2,
        },
        PostedPrice {
            resource: ResourceKind::Lenha,
            unit_price: 2,
        },
        PostedPrice {
            resource: ResourceKind::MetalBruto,
            unit_price: 5,
        },
        PostedPrice {
            resource: ResourceKind::Pao,
            unit_price: 4,
        },
        PostedPrice {
            resource: ResourceKind::Caldo,
            unit_price: 5,
        },
        PostedPrice {
            resource: ResourceKind::Ferramentas,
            unit_price: 9,
        },
        PostedPrice {
            resource: ResourceKind::Moedas,
            unit_price: 1,
        },
    ];

    let establishments = spatial
        .buildings
        .iter()
        .filter_map(|building| {
            let storage = spatial.fixtures.iter().find(|fixture| {
                fixture.kind == FixtureKind::Storage && fixture.building_id == Some(building.id)
            });
            let (stock_targets, wage_per_shift, public_service, default_stock, owner_household_ids) =
                match building.kind {
                    LocationKind::Farm => (
                        vec![
                            ResourceStack {
                                kind: ResourceKind::Graos,
                                amount: 18,
                            },
                            ResourceStack {
                                kind: ResourceKind::Ferramentas,
                                amount: 2,
                            },
                        ],
                        3,
                        false,
                        vec![ResourceStack {
                            kind: ResourceKind::Ferramentas,
                            amount: 1,
                        }],
                        role_households.get(&Role::Farmer).cloned().unwrap_or_default(),
                    ),
                    LocationKind::Woodlot => (
                        vec![ResourceStack {
                            kind: ResourceKind::Lenha,
                            amount: 16,
                        }],
                        3,
                        false,
                        vec![],
                        role_households.get(&Role::Farmer).cloned().unwrap_or_default(),
                    ),
                    LocationKind::Quarry => (
                        vec![ResourceStack {
                            kind: ResourceKind::MetalBruto,
                            amount: 12,
                        }],
                        4,
                        false,
                        vec![],
                        role_households.get(&Role::Farmer).cloned().unwrap_or_default(),
                    ),
                    LocationKind::Workshop => (
                        vec![
                            ResourceStack {
                                kind: ResourceKind::MetalBruto,
                                amount: 6,
                            },
                            ResourceStack {
                                kind: ResourceKind::Lenha,
                                amount: 6,
                            },
                            ResourceStack {
                                kind: ResourceKind::Ferramentas,
                                amount: 4,
                            },
                        ],
                        4,
                        false,
                        vec![
                            ResourceStack {
                                kind: ResourceKind::MetalBruto,
                                amount: 4,
                            },
                            ResourceStack {
                                kind: ResourceKind::Lenha,
                                amount: 4,
                            },
                        ],
                        role_households
                            .get(&Role::Blacksmith)
                            .cloned()
                            .unwrap_or_default(),
                    ),
                    LocationKind::Bakery => (
                        vec![
                            ResourceStack {
                                kind: ResourceKind::Graos,
                                amount: 10,
                            },
                            ResourceStack {
                                kind: ResourceKind::Lenha,
                                amount: 5,
                            },
                            ResourceStack {
                                kind: ResourceKind::Pao,
                                amount: 12,
                            },
                        ],
                        3,
                        false,
                        vec![ResourceStack {
                            kind: ResourceKind::Lenha,
                            amount: 4,
                        }],
                        role_households.get(&Role::Baker).cloned().unwrap_or_default(),
                    ),
                    LocationKind::Tavern => (
                        vec![
                            ResourceStack {
                                kind: ResourceKind::Graos,
                                amount: 6,
                            },
                            ResourceStack {
                                kind: ResourceKind::Lenha,
                                amount: 5,
                            },
                            ResourceStack {
                                kind: ResourceKind::Caldo,
                                amount: 10,
                            },
                        ],
                        3,
                        false,
                        vec![ResourceStack {
                            kind: ResourceKind::Lenha,
                            amount: 4,
                        }],
                        role_households
                            .get(&Role::TavernKeeper)
                            .cloned()
                            .unwrap_or_default(),
                    ),
                    LocationKind::GuardPost => (
                        vec![],
                        4,
                        true,
                        vec![],
                        Vec::new(),
                    ),
                    LocationKind::Manor => (
                        vec![],
                        5,
                        true,
                        vec![],
                        Vec::new(),
                    ),
                    _ => return None,
                };

            let mut stock = storage.map(|fixture| fixture.stock.clone()).unwrap_or_default();
            for stack in default_stock {
                merge_stack(&mut stock, stack);
            }

            let posted_prices = base_prices
                .iter()
                .filter(|price| stock_targets.iter().any(|target| target.kind == price.resource))
                .cloned()
                .collect::<Vec<_>>();

            Some(EstablishmentEconomy {
                id: building.id,
                building_id: Some(building.id),
                name: building.name.clone(),
                kind: building.kind,
                owner_household_ids,
                storage_fixture_id: storage.map(|fixture| fixture.id),
                cash: if public_service { 0 } else { 30 },
                stock,
                stock_targets,
                posted_prices,
                wage_per_shift,
                tool_wear: 0,
                public_service,
            })
        })
        .collect::<Vec<_>>();

    let village_economy = VillageEconomy {
        public_treasury: 140,
        daily_household_tax: 2,
        external_market_coord: TileCoord { x: 45, y: 13 },
        base_prices,
        external_quotes: vec![
            crate::world_model::ExternalMarketQuote {
                resource: ResourceKind::Lenha,
                buy_price: 3,
                sell_price: 1,
            },
            crate::world_model::ExternalMarketQuote {
                resource: ResourceKind::MetalBruto,
                buy_price: 7,
                sell_price: 4,
            },
            crate::world_model::ExternalMarketQuote {
                resource: ResourceKind::Graos,
                buy_price: 3,
                sell_price: 1,
            },
            crate::world_model::ExternalMarketQuote {
                resource: ResourceKind::Pao,
                buy_price: 5,
                sell_price: 2,
            },
            crate::world_model::ExternalMarketQuote {
                resource: ResourceKind::Caldo,
                buy_price: 6,
                sell_price: 2,
            },
            crate::world_model::ExternalMarketQuote {
                resource: ResourceKind::Ferramentas,
                buy_price: 10,
                sell_price: 6,
            },
        ],
        scarcity_metrics: Vec::new(),
    };

    (households, establishments, village_economy)
}

fn seeded_agents() -> Vec<SeedAgentTemplate> {
    let ids: Vec<u64> = (1..=12).collect();
    let names = vec![
        ("Alda", Role::Farmer),
        ("Breno", Role::Blacksmith),
        ("Celia", Role::Baker),
        ("Dario", Role::TavernKeeper),
        ("Elina", Role::Guard),
        ("Faro", Role::Headman),
        ("Gisa", Role::Farmer),
        ("Helmo", Role::Guard),
        ("Iria", Role::Baker),
        ("Joran", Role::Farmer),
        ("Kelda", Role::TavernKeeper),
        ("Lute", Role::Blacksmith),
    ];

    names
        .into_iter()
        .enumerate()
        .map(|(idx, (name, role))| {
            let id = (idx + 1) as u64;
            let relations = ids
                .iter()
                .copied()
                .filter(|other| *other != id)
                .map(|other| {
                    (
                        other,
                        AgentRelation {
                            trust: (((id * 7 + other * 3) % 36) as i32) - 10,
                            friendship: (((id * 5 + other * 2) % 26) as i32) - 5,
                            resentment: ((id * other) % 18) as i32,
                            attraction: (((id + other) * 3) % 14) as i32,
                            moral_debt: (((id * 2 + other) % 12) as i32) - 4,
                            reputation: (((id * 11 + other * 7) % 30) as i32) - 10,
                            last_updated_day: 1,
                            notes: Vec::new(),
                        },
                    )
                })
                .collect();

            SeedAgentTemplate {
                id,
                name: name.to_string(),
                role,
                profile: AgentProfile {
                    traits: vec!["observador".to_string(), "teimoso".to_string()],
                    values: vec!["honra".to_string(), "sobrevivencia".to_string()],
                    fears: vec!["escassez".to_string(), "humilhacao".to_string()],
                    long_term_desires: vec!["seguranca para a familia".to_string()],
                    moral_tolerances: vec!["mente por protecao".to_string()],
                    social_style: "prudente".to_string(),
                },
                state: AgentState {
                    mood: 55,
                    energy: 65,
                    health: 90,
                    hunger: 25,
                    stress: 18 + idx as i32,
                    current_focus: "manter rotina".to_string(),
                    active_goals: vec!["proteger reputacao".to_string()],
                },
                relations,
                memories: vec![
                    AgentMemory {
                        id: id * 100,
                        day: 1,
                        tick: 0,
                        kind: MemoryKind::Fact,
                        summary: format!("{name} conhece sua funcao social."),
                        details: format!(
                            "{name} entende as expectativas do papel de {}.",
                            role.as_str()
                        ),
                        emotional_weight: 10,
                        about: Vec::new(),
                        tags: vec!["papel".to_string(), "rotina".to_string()],
                    },
                ],
                inventory: Vec::new(),
                last_thought: format!("{name} mede o humor da vila antes de agir."),
            }
        })
        .collect()
}

fn fixture(
    kind: FixtureKind,
    x: i32,
    y: i32,
    name: &'static str,
    blocks_movement: bool,
    stock: Vec<ResourceStack>,
) -> FixturePlacement {
    FixturePlacement {
        kind,
        coord: TileCoord { x, y },
        name,
        blocks_movement,
        stock,
    }
}

fn merge_stack(stacks: &mut Vec<ResourceStack>, stack: ResourceStack) {
    if let Some(existing) = stacks
        .iter_mut()
        .find(|existing| existing.kind == stack.kind)
    {
        existing.amount += stack.amount;
    } else {
        stacks.push(stack);
    }
}

fn consume_matching(stacks: &mut Vec<ResourceStack>, accepted: &[ResourceKind]) -> bool {
    for stack in stacks.iter_mut() {
        if accepted.contains(&stack.kind) && stack.amount > 0 {
            stack.amount -= 1;
            return true;
        }
    }
    false
}

fn reconstruct_path(
    start: TileCoord,
    goal: TileCoord,
    came_from: &HashMap<TileCoord, TileCoord>,
) -> Vec<TileCoord> {
    let mut current = goal;
    let mut path = vec![goal];
    while current != start {
        current = came_from[&current];
        if current != start {
            path.push(current);
        }
    }
    path.reverse();
    path
}

fn invert_delta(delta: &RelationDelta) -> RelationDelta {
    RelationDelta {
        trust: delta.trust / 2,
        friendship: delta.friendship / 2,
        resentment: delta.resentment,
        attraction: delta.attraction / 2,
        moral_debt: -delta.moral_debt,
        reputation: delta.reputation / 2,
    }
}

fn social_goal_from_move(move_kind: SocialMove) -> &'static str {
    match move_kind {
        SocialMove::Chat => "medir o humor do outro",
        SocialMove::Gossip => "trocar rumores uteis",
        SocialMove::Promise => "firmar um compromisso",
        SocialMove::Offend => "pressionar e descarregar frustracao",
        SocialMove::Reconcile => "reparar a relacao",
        SocialMove::Favor => "oferecer ajuda e aproximacao",
    }
}

fn other_participant(participants: &[u64; 2], current: u64) -> u64 {
    if participants[0] == current {
        participants[1]
    } else {
        participants[0]
    }
}

fn extend_summary(current: &str, addition: &str) -> String {
    let candidate = if current.is_empty() {
        addition.to_string()
    } else {
        format!("{current} | {addition}")
    };
    let chars = candidate.chars().collect::<Vec<_>>();
    if chars.len() <= 320 {
        candidate
    } else {
        chars[chars.len() - 320..].iter().collect()
    }
}

struct SpatialBuilder {
    grid: WorldGrid,
    buildings: Vec<BuildingSpec>,
    rooms: Vec<RoomSpec>,
    fixtures: Vec<FixtureSpec>,
    next_building_id: u64,
    next_room_id: u64,
    next_fixture_id: u64,
}

impl SpatialBuilder {
    fn new(width: i32, height: i32) -> Self {
        Self {
            grid: WorldGrid {
                width,
                height,
                tiles: Vec::new(),
            },
            buildings: Vec::new(),
            rooms: Vec::new(),
            fixtures: Vec::new(),
            next_building_id: 1,
            next_room_id: 1,
            next_fixture_id: 1,
        }
    }

    fn fill(&mut self, kind: TileKind) {
        self.grid.tiles.clear();
        for y in 0..self.grid.height {
            for x in 0..self.grid.width {
                self.grid.tiles.push(TileSpec {
                    coord: TileCoord { x, y },
                    kind,
                    building_id: None,
                    room_id: None,
                });
            }
        }
    }

    fn carve_road_rect(&mut self, rect: Rect) {
        for coord in rect.footprint() {
            self.set_tile(coord, TileKind::Road, None, None);
        }
    }

    fn carve_field_rect(&mut self, rect: Rect) {
        for coord in rect.footprint() {
            self.set_tile(coord, TileKind::Field, None, None);
        }
    }

    fn carve_terrain_rect(&mut self, rect: Rect, kind: TileKind) {
        for coord in rect.footprint() {
            self.set_tile(coord, kind, None, None);
        }
    }

    fn carve_road_line(&mut self, start: TileCoord, end: TileCoord) {
        if start.x == end.x {
            let min_y = start.y.min(end.y);
            let max_y = start.y.max(end.y);
            for y in min_y..=max_y {
                self.set_tile(TileCoord { x: start.x, y }, TileKind::Road, None, None);
            }
        } else if start.y == end.y {
            let min_x = start.x.min(end.x);
            let max_x = start.x.max(end.x);
            for x in min_x..=max_x {
                self.set_tile(TileCoord { x, y: start.y }, TileKind::Road, None, None);
            }
        }
    }

    fn add_building(
        &mut self,
        name: &'static str,
        kind: LocationKind,
        rect: Rect,
        entrance: TileCoord,
        room_name: &'static str,
        room_kind: &'static str,
        fixtures: Vec<FixturePlacement>,
    ) -> BuildingId {
        let building_id = self.next_building_id;
        self.next_building_id += 1;
        let room_id = self.next_room_id;
        self.next_room_id += 1;

        for coord in rect.border_tiles() {
            self.set_tile(coord, TileKind::Wall, Some(building_id), Some(room_id));
        }
        for coord in rect.interior_tiles() {
            self.set_tile(coord, TileKind::Floor, Some(building_id), Some(room_id));
        }
        self.set_tile(entrance, TileKind::Door, Some(building_id), Some(room_id));

        self.rooms.push(RoomSpec {
            id: room_id,
            building_id,
            name: room_name.to_string(),
            kind: room_kind.to_string(),
            tiles: rect.interior_tiles(),
        });

        self.buildings.push(BuildingSpec {
            id: building_id,
            name: name.to_string(),
            kind,
            entrance,
            room_ids: vec![room_id],
            footprint: rect.footprint(),
        });

        for placement in fixtures {
            self.fixtures.push(FixtureSpec {
                id: self.next_fixture_id,
                building_id: Some(building_id),
                room_id: Some(room_id),
                kind: placement.kind,
                coord: placement.coord,
                name: placement.name.to_string(),
                blocks_movement: placement.blocks_movement,
                stock: placement.stock,
            });
            self.next_fixture_id += 1;
        }

        building_id
    }

    fn add_outdoor_fixture(
        &mut self,
        building_id: Option<BuildingId>,
        room_id: Option<RoomId>,
        kind: FixtureKind,
        coord: TileCoord,
        name: &'static str,
        blocks_movement: bool,
        stock: Vec<ResourceStack>,
    ) {
        self.fixtures.push(FixtureSpec {
            id: self.next_fixture_id,
            building_id,
            room_id,
            kind,
            coord,
            name: name.to_string(),
            blocks_movement,
            stock,
        });
        self.next_fixture_id += 1;
    }

    fn set_tile(
        &mut self,
        coord: TileCoord,
        kind: TileKind,
        building_id: Option<BuildingId>,
        room_id: Option<RoomId>,
    ) {
        let index = (coord.y * self.grid.width + coord.x) as usize;
        if let Some(tile) = self.grid.tiles.get_mut(index) {
            tile.kind = kind;
            tile.building_id = building_id;
            tile.room_id = room_id;
        }
    }

    fn finish(self) -> SpatialSnapshot {
        SpatialSnapshot {
            grid: self.grid,
            buildings: self.buildings,
            rooms: self.rooms,
            fixtures: self.fixtures,
        }
    }
}
