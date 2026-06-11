use crate::agent_mind::{
    ConversationContextInput, ConversationObservedAgentInput,
    ConversationTurnInput, ConversationTurnOutput, DecisionInput,
    EconomicContextInput, EconomicOpportunityInput, LegalContextInput, NearbyAgentInput,
    NearbyFixtureInput, PoliticalContextInput, PsychologicalContextInput, RecentEventInput,
    RelationalHistoryInput, retrieve_relational_memories, retrieve_relevant_memories,
    validate_intent, ThinkMakerInput, ThinkMakerOutput, parse_action_planner_output,
};
use crate::economy_catalog::{default_economy_catalog, validate_catalog};
use crate::llm_adapter::{LlmAdapter, LlmError};
use crate::world_model::{
    AgentIntent, AgentLifeStatus, AgentMemory, AgentProfile, AgentRelation, AgentSnapshot,
    AgentState, BuildingId, BuildingSpec, CombatId, CombatOutcome, CombatState, CombatStatus,
    ConversationId, ConversationOutcome, ConversationParticipantState, ConversationState,
    ConversationStatus, ConversationTurn, CrimeCase, CrimeCaseId, CrimeCaseStatus, CrimeType,
    EconomicNode, EconomicTask, EconomicTaskClass, EconomicTaskId, EconomicTaskKind,
    EconomicTaskPhase, EconomyCatalog, EstablishmentEconomy, EstablishmentId, EventKind, FixtureId,
    FixtureKind, FixtureSpec, HouseholdEconomy, InjuryState, IntentKind, JusticeSeverity,
    LocalNorms, LocationKind, MemoryKind, PendingPaymentClaim, PolicyDomain, PoliticalFaction,
    PoliticalFactionId, PoliticalIssue, PoliticalIssueId, PoliticalIssueStatus, PoliticalPressure,
    PostedPrice, RationingPolicy, RelationDelta, ResourceKind, ResourceStack, Role, RoomId,
    ScarcityMetric, SentenceKind, SimplifiedTask, SimulationSnapshot, SocialMove,
    SpatialSnapshot, TileCoord, TileKind, TileSpec, TraumaTracker, VillageEconomy, WorldEvent,
    CropStage, CropState, FactionObjective,
};
use anyhow::{Result, anyhow};
use bevy_ecs::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

const SNAPSHOT_SCHEMA_VERSION: u32 = 10;
pub const SIMULATED_MINUTES_PER_TICK: u32 = 1;
pub const DEFAULT_TICKS_PER_DAY: u32 = 24 * 60 / SIMULATED_MINUTES_PER_TICK;
pub const DEFAULT_TICKS_PER_SECOND: u32 = 1;
pub const MAX_TICKS_PER_SECOND: u32 = 10;
const MAX_CONVERSATION_TURNS: u32 = 6;
const CONVERSATION_RECENT_TURNS_LIMIT: usize = 6;
const ROUTINE_RECONSIDERATION_MAX: u32 = 4;

const BLOCKED_RECONSIDERATION_TICKS: u32 = 2;
const DEFAULT_CARRYING_CAPACITY: i32 = 4;

pub fn tick_interval_ms(ticks_per_second: u32) -> u64 {
    1_000 / u64::from(ticks_per_second.max(1))
}

#[derive(Component, Clone)]
pub struct AgentCore {
    pub id: u64,
    pub name: String,
    pub role_id: String,
    pub home_building_id: Option<BuildingId>,
    pub work_building_id: Option<BuildingId>,
    pub home_bed: Option<TileCoord>,
}

#[derive(Component, Clone)]
pub struct ProfileComponent(pub AgentProfile);

#[derive(Component, Clone)]
pub struct StateComponent(pub AgentState);

#[derive(Component, Clone, Default)]
pub struct LifeStatusComponent(pub AgentLifeStatus);

#[derive(Component, Clone, Default)]
pub struct InjuryComponent(pub InjuryState);

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
pub struct TaskQueueComponent(pub VecDeque<SimplifiedTask>);

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

#[derive(Component, Clone, Default)]
pub struct TraumaTrackerComponent(pub TraumaTracker);

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
    pub num_villages: usize,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            village_name: "Santa Bruma".to_string(),
            ticks_per_day: DEFAULT_TICKS_PER_DAY,
            max_agents: 21,
            relevant_memory_limit: 5,
            recent_event_limit: 6,
            grid_width: 150,
            grid_height: 100,
            world_seed: 1,
            num_villages: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentView {
    pub id: u64,
    pub name: String,
    pub role_id: String,
    pub role_name: String,
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
    pub life_status: AgentLifeStatus,
    pub injury: InjuryState,
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
    pub political_position: String,
    pub political_grievances: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MapRender {
    pub rows: Vec<String>,
}

#[derive(Debug)]
pub struct CompletedThoughts {
    pub agent_id: u64,
    pub output: ThinkMakerOutput,
}

#[derive(Debug)]
pub struct SkippedThoughts {
    pub agent_id: u64,
    pub error: LlmError,
}

#[derive(Debug)]
pub enum ThinkMakerResult {
    Completed(CompletedThoughts),
    Skipped(SkippedThoughts),
}

struct PendingThoughts {
    agent_id: u64,
    handle: std::thread::JoinHandle<ThinkMakerResult>,
}

pub struct Simulation {
    catalog: EconomyCatalog,
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
    next_combat_id: CombatId,
    combats: Vec<CombatState>,
    next_crime_case_id: CrimeCaseId,
    crime_cases: Vec<CrimeCase>,
    next_political_faction_id: PoliticalFactionId,
    next_political_issue_id: PoliticalIssueId,
    political_factions: Vec<PoliticalFaction>,
    political_issues: Vec<PoliticalIssue>,
    political_pressures: Vec<PoliticalPressure>,
    local_norms: LocalNorms,
    next_economic_task_id: EconomicTaskId,
    households: Vec<HouseholdEconomy>,
    establishments: Vec<EstablishmentEconomy>,
    village_economy: VillageEconomy,
    economic_tasks: Vec<EconomicTask>,
    pending_thoughts: Vec<PendingThoughts>,
    pub crops: HashMap<TileCoord, CropState>,
}

impl Drop for Simulation {
    fn drop(&mut self) {
        for pending in self.pending_thoughts.drain(..) {
            let _ = pending.handle.join();
        }
    }
}

impl Simulation {
    pub fn seeded(config: SimulationConfig) -> Self {
        let snapshot = crate::world_gen::generate_world(config).expect("Erro ao gerar o mundo");
        Self::from_snapshot(snapshot)
    }

    pub fn from_snapshot(snapshot: SimulationSnapshot) -> Self {
        let catalog = default_economy_catalog();
        validate_catalog(&catalog).expect("default economy catalog should be valid");
        let mut world = World::new();
        let conversations = snapshot.conversations.clone();
        let next_conversation_id = snapshot.next_conversation_id;
        let combats = snapshot.combats.clone();
        let crime_cases = snapshot.crime_cases.clone();
        let political_factions = snapshot.political_factions.clone();
        let political_issues = snapshot.political_issues.clone();
        let political_pressures = snapshot.political_pressures.clone();
        let local_norms = snapshot.local_norms.clone();
        for agent in snapshot.agents {
            world.spawn((
                AgentCore {
                    id: agent.id,
                    name: agent.name,
                    role_id: agent.role_id,
                    home_building_id: agent.home_building_id,
                    work_building_id: agent.work_building_id,
                    home_bed: agent.home_bed,
                },
                ProfileComponent(agent.profile),
                StateComponent(agent.state),
                LifeStatusComponent(agent.life_status),
                InjuryComponent(agent.injury),
                RelationComponent(agent.relations),
                MemoryComponent(agent.memories),
                InventoryComponent(agent.inventory),
                PositionComponent(agent.position),
                DestinationComponent(agent.destination),
                DestinationLabelComponent(agent.destination_label),
                PathComponent(agent.planned_path),
                IntentComponent(agent.last_intent),
                TaskQueueComponent(agent.task_queue.into()),
                (
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
                    TraumaTrackerComponent(agent.trauma_tracker),
                ),
            ));
        }

        Self {
            catalog,
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
            next_combat_id: snapshot.next_combat_id,
            combats,
            next_crime_case_id: snapshot.next_crime_case_id,
            crime_cases,
            next_political_faction_id: snapshot.next_political_faction_id,
            next_political_issue_id: snapshot.next_political_issue_id,
            political_factions,
            political_issues,
            political_pressures,
            local_norms,
            next_economic_task_id: snapshot.next_economic_task_id,
            households: snapshot.households,
            establishments: snapshot.establishments,
            village_economy: snapshot.village_economy,
            economic_tasks: snapshot.economic_tasks,
            pending_thoughts: Vec::new(),
            crops: snapshot.crops,
        }
    }

    pub fn tick(&mut self, llm: &dyn LlmAdapter) -> Result<()> {
        self.total_ticks += 1;
        self.tick_of_day += 1;
        
        // Crescimento das plantações
        for crop in self.crops.values_mut() {
            crop.ticks_since_planted += 1;
            if crop.ticks_since_planted >= 30 {
                crop.stage = CropStage::Ready;
            } else if crop.ticks_since_planted >= 10 {
                crop.stage = CropStage::Growing;
            }
        }

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
        self.refresh_political_state()?;
        self.apply_faction_action_overrides()?;
        let agent_ids = self.agent_ids();

        for agent_id in &agent_ids {
            if self.can_agent_act(*agent_id)? {
                self.advance_agent_movement(*agent_id)?;
            }
        }

        for agent_id in &agent_ids {
            if self.can_agent_act(*agent_id)? {
                self.ensure_navigation_for_current_intent(*agent_id)?;
            }
        }

        for agent_id in &agent_ids {
            if self.can_agent_act(*agent_id)? {
                self.try_execute_current_intent(*agent_id, llm)?;
            }
        }

        self.process_active_conversations(llm)?;
        self.process_general_decisions(llm)?;
        self.update_trauma_trackers()?;

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
            let state = entry
                .get::<StateComponent>()
                .expect("missing state component");
            let life_status = entry
                .get::<LifeStatusComponent>()
                .expect("missing life status component");
            let injury = entry
                .get::<InjuryComponent>()
                .expect("missing injury component");
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
            let path = entry
                .get::<PathComponent>()
                .expect("missing path component");
            let intent = entry
                .get::<IntentComponent>()
                .expect("missing intent component");
            let task_queue = entry
                .get::<TaskQueueComponent>()
                .expect("missing task queue component");
            let thought = entry
                .get::<ThoughtComponent>()
                .expect("missing thought component");
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
                role_id: core.role_id.clone(),
                home_building_id: core.home_building_id,
                work_building_id: core.work_building_id,
                home_bed: core.home_bed,
                profile: profile.0.clone(),
                state: state.0.clone(),
                life_status: life_status.0,
                injury: injury.0.clone(),
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
                task_queue: task_queue.0.iter().cloned().collect(),
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
                trauma_tracker: entry
                    .get::<TraumaTrackerComponent>()
                    .map(|t| t.0.clone())
                    .unwrap_or_default(),
            });
        }

        SimulationSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            catalog_version: self.catalog.version,
            village_name: self.village_name.clone(),
            day: self.day,
            tick_of_day: self.tick_of_day,
            total_ticks: self.total_ticks,
            ticks_per_day: self.ticks_per_day,
            next_memory_id: self.next_memory_id,
            next_conversation_id: self.next_conversation_id,
            next_economic_task_id: self.next_economic_task_id,
            next_combat_id: self.next_combat_id,
            next_crime_case_id: self.next_crime_case_id,
            next_political_faction_id: self.next_political_faction_id,
            next_political_issue_id: self.next_political_issue_id,
            agents,
            conversations: self.conversations.clone(),
            combats: self.combats.clone(),
            crime_cases: self.crime_cases.clone(),
            political_factions: self.political_factions.clone(),
            political_issues: self.political_issues.clone(),
            political_pressures: self.political_pressures.clone(),
            local_norms: self.local_norms.clone(),
            households: self.households.clone(),
            establishments: self.establishments.clone(),
            village_economy: self.village_economy.clone(),
            economic_tasks: self.economic_tasks.clone(),
            spatial: self.spatial.clone(),
            events: self.events.clone(),
            crops: self.crops.clone(),
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

    fn role_display_name(&self, role_id: &str) -> String {
        self.catalog
            .roles
            .iter()
            .find(|role| role.id == role_id)
            .map(|role| role.display_name.clone())
            .unwrap_or_else(|| role_id.to_string())
    }

    fn resource_display_name(&self, resource_id: &str) -> String {
        self.catalog
            .resources
            .iter()
            .find(|resource| resource.id == resource_id)
            .map(|resource| resource.display_name.clone())
            .unwrap_or_else(|| resource_id.to_string())
    }

    fn resource_def(&self, resource_id: &str) -> Option<&crate::world_model::ResourceDef> {
        self.catalog
            .resources
            .iter()
            .find(|resource| resource.id == resource_id)
    }

    fn role_def(&self, role_id: &str) -> Option<&crate::world_model::RoleDef> {
        self.catalog.roles.iter().find(|role| role.id == role_id)
    }

    fn establishment_type_def(
        &self,
        establishment_type_id: &str,
    ) -> Option<&crate::world_model::EstablishmentTypeDef> {
        self.catalog
            .establishment_types
            .iter()
            .find(|entry| entry.id == establishment_type_id)
    }

    fn recipe_for_establishment_type(
        &self,
        establishment_type_id: &str,
    ) -> Option<&crate::world_model::RecipeDef> {
        self.catalog.recipes.iter().find(|recipe| {
            recipe.establishment_type_id == establishment_type_id
                && self
                    .establishment_type_def(establishment_type_id)
                    .and_then(|entry| entry.production_recipe_id.as_ref())
                    .map(|recipe_id| recipe_id == &recipe.id)
                    .unwrap_or(false)
        })
    }

    fn recipe_for_establishment(
        &self,
        establishment: &EstablishmentEconomy,
    ) -> Option<&crate::world_model::RecipeDef> {
        self.recipe_for_establishment_type(&establishment.establishment_type_id)
    }

    fn market_quote(&self, resource_id: &str) -> Option<&crate::world_model::ExternalMarketQuote> {
        self.village_economy
            .external_quotes
            .iter()
            .find(|quote| quote.resource_id == resource_id)
    }

    fn stock_target_amount(&self, establishment: &EstablishmentEconomy, resource_id: &str) -> i32 {
        establishment
            .stock_targets
            .iter()
            .find(|target| target.resource_id == resource_id)
            .map(|target| target.amount)
            .unwrap_or(0)
    }

    fn is_food_resource(&self, resource_id: &str) -> bool {
        self.resource_def(resource_id)
            .map(|resource| resource.tags.iter().any(|tag| tag == "food"))
            .unwrap_or(false)
    }

    fn food_resource_ids_sorted(&self) -> Vec<String> {
        let mut resources = self
            .catalog
            .resources
            .iter()
            .filter(|resource| resource.tags.iter().any(|tag| tag == "food"))
            .map(|resource| (resource.consumption_priority, resource.id.clone()))
            .collect::<Vec<_>>();
        resources.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        resources.into_iter().map(|(_, id)| id).collect()
    }

    pub fn economy_overview(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "caixa_publico={} | imposto_diario_por_lar={}",
            self.village_economy.public_treasury, self.village_economy.daily_household_tax
        )];
        for establishment in self.establishments.iter().filter(|establishment| {
            establishment.public_service || self.recipe_for_establishment(establishment).is_some()
        }) {
            let stock = establishment
                .stock
                .iter()
                .map(|stack| {
                    format!(
                        "{}x{}",
                        self.resource_display_name(&stack.resource_id),
                        stack.amount
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "{} | caixa={} | {}",
                establishment.name, establishment.cash, stock
            ));
        }
        lines
    }

    pub fn legal_overview(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for case in self.crime_cases.iter().rev().take(6) {
            lines.push(format!(
                "caso #{} {:?} status={:?} suspeito={:?} vitima={:?} severidade={} confianca={} sentenca={:?}",
                case.id,
                case.crime_type,
                case.status,
                case.suspect_id,
                case.victim_id,
                case.severity,
                case.confidence,
                case.sentence
            ));
        }
        for combat in self
            .combats
            .iter()
            .filter(|combat| combat.status == CombatStatus::Active)
            .take(4)
        {
            lines.push(format!(
                "combate #{} {:?} participantes={:?} round={}",
                combat.id, combat.status, combat.participants, combat.round
            ));
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
            &LifeStatusComponent,
            &InjuryComponent,
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
            life_status,
            injury,
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
                .map(|entry| {
                    entry
                        .pending_payments
                        .iter()
                        .map(|claim| claim.amount)
                        .sum()
                })
                .unwrap_or(0);
            let work_establishment = core
                .work_building_id
                .and_then(|building_id| self.establishment_by_building(building_id));
            views.push(AgentView {
                id: core.id,
                name: core.name.clone(),
                role_id: core.role_id.clone(),
                role_name: self.role_display_name(&core.role_id),
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
                life_status: life_status.0,
                injury: injury.0.clone(),
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
                political_position: self.political_position_for_agent(core.id),
                political_grievances: self.political_grievances_for_agent(core.id),
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

                if let Some(crop) = self.crops.get(&coord) {
                    ch = match crop.stage {
                        CropStage::Planted => '.',
                        CropStage::Growing => 'v',
                        CropStage::Ready => 'Y',
                    };
                }

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
                    } else if self
                        .life_status(*agent_id)
                        .map(|status| status == AgentLifeStatus::Morto)
                        .unwrap_or(false)
                    {
                        'x'
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

    pub fn debug_force_agent_state(&mut self, agent_id: u64, state: AgentState) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<StateComponent>()
            .ok_or_else(|| anyhow!("missing state component"))?
            .0 = state;
        Ok(())
    }

    pub fn debug_assign_intent(&mut self, agent_id: u64, intent: AgentIntent) -> Result<()> {
        self.pending_thoughts
            .retain(|pending| pending.agent_id != agent_id);
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0 = Some(intent.clone());
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
        if !matches!(
            intent.kind,
            IntentKind::Trabalhar
                | IntentKind::Comprar
                | IntentKind::Transportar
                | IntentKind::Vender
                | IntentKind::ReceberPagamento
        ) {
            entity_mut
                .get_mut::<EconomicActivityComponent>()
                .ok_or_else(|| anyhow!("missing economy component"))?
                .active_task_id = None;
            for task in self.economic_tasks.iter_mut().filter(|task| {
                task.assigned_agent_id == Some(agent_id)
                    && !matches!(
                        task.phase,
                        EconomicTaskPhase::Completed | EconomicTaskPhase::Failed
                    )
            }) {
                task.assigned_agent_id = None;
            }
        }
        Ok(())
    }

    pub fn debug_force_navigation(
        &mut self,
        agent_id: u64,
        destination: TileCoord,
        path: Vec<TileCoord>,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = Some(destination);
        entity_mut
            .get_mut::<DestinationLabelComponent>()
            .ok_or_else(|| anyhow!("missing destination label component"))?
            .0 = Some("debug".to_string());
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0 = path;
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

    pub fn debug_set_public_treasury(&mut self, amount: i32) {
        self.village_economy.public_treasury = amount.max(0);
    }

    pub fn debug_set_household_tax_arrears(
        &mut self,
        household_id: BuildingId,
        arrears: i32,
    ) -> Result<()> {
        let Some(household) = self.household_by_id_mut(household_id) else {
            return Err(anyhow!("household {household_id} not found"));
        };
        household.tax_arrears = arrears.max(0);
        Ok(())
    }

    pub fn debug_refresh_politics(&mut self) -> Result<()> {
        self.refresh_political_state()
    }

    pub fn debug_resolve_daily_politics(&mut self) -> Result<()> {
        self.resolve_daily_politics()
    }

    fn apply_needs_decay(&mut self) {
        let mut death_candidates = Vec::new();
        let mut query = self.world.query::<(
            &AgentCore,
            &LifeStatusComponent,
            &mut InjuryComponent,
            &mut StateComponent,
        )>();
        for (core, life_status, mut injury, mut state) in query.iter_mut(&mut self.world) {
            if life_status.0 == AgentLifeStatus::Morto {
                continue;
            }
            if injury.0.bleeding > 0 {
                state.0.health = (state.0.health - injury.0.bleeding).clamp(0, 100);
            }
            if injury.0.recovery_ticks > 0 {
                injury.0.recovery_ticks -= 1;
                if injury.0.recovery_ticks == 0 {
                    injury.0.pain = (injury.0.pain - 8).clamp(0, 100);
                    injury.0.bleeding = (injury.0.bleeding - 1).clamp(0, 100);
                }
            }
            if (self.total_ticks.wrapping_add(core.id)) % 10 == 0 {
                state.0.hunger = (state.0.hunger + 1).clamp(0, 100);
            }
            if (self.total_ticks.wrapping_add(core.id)) % 20 == 0 {
                state.0.energy = (state.0.energy - 1).clamp(0, 100);
            }
            state.0.stress = (state.0.stress + 1).clamp(0, 100);
            if state.0.hunger > 90 || state.0.energy < 10 {
                state.0.health = (state.0.health - 1).clamp(0, 100);
            }
            if state.0.health <= 0 {
                death_candidates.push(core.id);
            }
        }
        for agent_id in death_candidates {
            let _ = self.mark_agent_dead(agent_id, "colapso fisico");
        }
    }

    fn can_agent_act(&mut self, agent_id: u64) -> Result<bool> {
        let entity = self.find_agent_entity(agent_id)?;
        let life_status = self
            .world
            .entity(entity)
            .get::<LifeStatusComponent>()
            .ok_or_else(|| anyhow!("missing life status component"))?
            .0;
        Ok(life_status == AgentLifeStatus::Vivo)
    }

    fn apply_emergency_food_rule(&mut self, agent_id: u64) -> Result<bool> {
        let state = self.agent_state(agent_id)?;
        if state.hunger < 70 {
            return Ok(false);
        }
        if self.household_has_food_available(agent_id)? {
            self.apply_eat(agent_id)?;
            return Ok(true);
        }

        if let Some(task) = self.active_economic_task_for_agent(agent_id)
            && matches!(
                task.kind,
                EconomicTaskKind::Comprar
                    | EconomicTaskKind::Transportar
                    | EconomicTaskKind::ReceberPagamento
            )
        {
            return Ok(false);
        }

        let eat_intent = AgentIntent {
            kind: IntentKind::Comer,
            target_agent: None,
            target_semantic: Some("comida da despensa".to_string()),
            justification: "Fome critica exige resolver alimentacao antes de qualquer plano."
                .to_string(),
            dominant_emotion: "urgencia".to_string(),
            perceived_risk: 9,
            belief_updates: vec!["A fome passou a dominar a prioridade imediata.".to_string()],
            priority: 10,
            social_move: None,
        };
        let entity = self.find_agent_entity(agent_id)?;
        {
            let mut entity_mut = self.world.entity_mut(entity);
            entity_mut
                .get_mut::<IntentComponent>()
                .ok_or_else(|| anyhow!("missing intent component"))?
                .0 = Some(eat_intent.clone());
            entity_mut
                .get_mut::<ThoughtComponent>()
                .ok_or_else(|| anyhow!("missing thought component"))?
                .0 = "Fome critica: priorizando comida ou compra de alimento.".to_string();
            entity_mut
                .get_mut::<DestinationComponent>()
                .ok_or_else(|| anyhow!("missing destination component"))?
                .0 = None;
            entity_mut
                .get_mut::<DestinationLabelComponent>()
                .ok_or_else(|| anyhow!("missing destination label component"))?
                .0 = eat_intent.target_semantic.clone();
            entity_mut
                .get_mut::<PathComponent>()
                .ok_or_else(|| anyhow!("missing path component"))?
                .0
                .clear();
        }
        if self.reroute_eat_intent_to_food_purchase(agent_id)? {
            self.ensure_navigation_for_current_intent(agent_id)?;
            return Ok(true);
        }
        // Reroute falhou (sem oferta viavel) — limpar intent para que o
        // motor autonomo decida a proxima acao (trabalhar, coletar pagamento, etc).
        self.clear_intent_navigation(agent_id)?;
        Ok(false)
    }

    /// Motor econômico autônomo — regras determinísticas de sobrevivência e produção.
    /// Chamado quando o agente não possui intent (aguardando LLM ou recém-criado).
    /// Retorna Some(intent) se atribuiu um comportamento, None caso contrário.
    fn apply_survival_economy(&mut self, agent_id: u64) -> Result<Option<AgentIntent>> {
        let state = self.agent_state(agent_id)?;
        let hunger = state.hunger;
        let energy = state.energy;
        let household_id = self.household_id_for_agent(agent_id);
        let has_pending_payments = household_id
            .and_then(|id| self.household_by_id(id))
            .map(|h| !h.pending_payments.is_empty())
            .unwrap_or(false);
        let can_afford_food = household_id
            .and_then(|id| self.best_food_source_for_household(id))
            .is_some();

        // ── Prioridade 1: Comer se com fome e despensa tem comida ────────
        if hunger >= 50 && self.household_has_food_available(agent_id)? {
            let intent = AgentIntent {
                kind: IntentKind::Comer,
                target_agent: None,
                target_semantic: Some("comida da despensa".to_string()),
                justification: "Motor autonomo: fome detectada, comida disponivel na despensa."
                    .to_string(),
                dominant_emotion: "fome".to_string(),
                perceived_risk: 0,
                belief_updates: Vec::new(),
                priority: 8,
                social_move: None,
            };
            self.set_autopilot_intent(agent_id, &intent, "Fome: indo comer da despensa.")?;
            return Ok(Some(intent));
        }

        // ── Prioridade 2: Comprar comida (só se tem dinheiro suficiente) ──
        if hunger >= 45 && !self.household_has_food_available(agent_id)? && can_afford_food {
            let purchase_intent = AgentIntent {
                kind: IntentKind::Comprar,
                target_agent: None,
                target_semantic: Some("comida para a despensa".to_string()),
                justification: "Motor autonomo: despensa vazia, procurando comida para comprar."
                    .to_string(),
                dominant_emotion: "urgencia".to_string(),
                perceived_risk: 0,
                belief_updates: Vec::new(),
                priority: 9,
                social_move: None,
            };
            self.ensure_economic_tasks();
            self.bind_or_create_economic_task(agent_id, &purchase_intent)?;
            let task_found = self
                .active_economic_task_for_agent(agent_id)
                .map(|task| task.kind == EconomicTaskKind::Comprar)
                .unwrap_or(false);
            if task_found {
                let task_desc = self
                    .active_economic_task_for_agent(agent_id)
                    .map(|task| task.description.clone())
                    .unwrap_or_default();
                let intent = AgentIntent {
                    kind: IntentKind::Comprar,
                    target_agent: None,
                    target_semantic: Some(task_desc.clone()),
                    justification: format!("Motor autonomo: comprando comida — {}", task_desc),
                    dominant_emotion: "urgencia".to_string(),
                    perceived_risk: 0,
                    belief_updates: Vec::new(),
                    priority: 9,
                    social_move: None,
                };
                self.set_autopilot_intent(
                    agent_id,
                    &intent,
                    &format!("Despensa vazia: {}", task_desc),
                )?;
                return Ok(Some(intent));
            }
        }

        // ── Prioridade 3: Coletar pagamentos pendentes ───────────────────
        // Se o lar tem pending_payments (salários), um membro vai buscar.
        if has_pending_payments {
            let payment_intent = AgentIntent {
                kind: IntentKind::ReceberPagamento,
                target_agent: None,
                target_semantic: Some("pagamentos pendentes".to_string()),
                justification: "Motor autonomo: salarios pendentes para recolher.".to_string(),
                dominant_emotion: "determinado".to_string(),
                perceived_risk: 0,
                belief_updates: Vec::new(),
                priority: 8,
                social_move: None,
            };
            self.ensure_economic_tasks();
            self.bind_or_create_economic_task(agent_id, &payment_intent)?;
            let task_found = self
                .active_economic_task_for_agent(agent_id)
                .map(|task| task.kind == EconomicTaskKind::ReceberPagamento)
                .unwrap_or(false);
            if task_found {
                let task_desc = self
                    .active_economic_task_for_agent(agent_id)
                    .map(|task| task.description.clone())
                    .unwrap_or_default();
                let intent = AgentIntent {
                    kind: IntentKind::ReceberPagamento,
                    target_agent: None,
                    target_semantic: Some(task_desc.clone()),
                    justification: format!("Motor autonomo: {}", task_desc),
                    dominant_emotion: "determinado".to_string(),
                    perceived_risk: 0,
                    belief_updates: Vec::new(),
                    priority: 8,
                    social_move: None,
                };
                self.set_autopilot_intent(agent_id, &intent, &format!("Salario: {}", task_desc))?;
                return Ok(Some(intent));
            }
        }

        // ── Prioridade 4: Descansar se energia crítica ───────────────────
        if energy <= 15 {
            let intent = AgentIntent {
                kind: IntentKind::Descansar,
                target_agent: None,
                target_semantic: Some("cama de casa".to_string()),
                justification: "Motor autonomo: energia critica, precisa descansar.".to_string(),
                dominant_emotion: "exaustao".to_string(),
                perceived_risk: 0,
                belief_updates: Vec::new(),
                priority: 7,
                social_move: None,
            };
            self.set_autopilot_intent(agent_id, &intent, "Exausto: indo descansar.")?;
            return Ok(Some(intent));
        }

        // ── Prioridade 5: Trabalho produtivo baseado no papel ────────────
        {
            let work_intent = AgentIntent {
                kind: IntentKind::Trabalhar,
                target_agent: None,
                target_semantic: Some("trabalho".to_string()),
                justification: "Motor autonomo: sem tarefas pendentes, buscando trabalho."
                    .to_string(),
                dominant_emotion: "determinado".to_string(),
                perceived_risk: 0,
                belief_updates: Vec::new(),
                priority: 5,
                social_move: None,
            };
            self.ensure_economic_tasks();
            self.bind_or_create_economic_task(agent_id, &work_intent)?;
            if let Some(task) = self.active_economic_task_for_agent(agent_id).cloned() {
                let intent = Self::intent_for_economic_task(&task);
                self.set_autopilot_intent(
                    agent_id,
                    &intent,
                    &format!("Trabalho: {}", task.description),
                )?;
                return Ok(Some(intent));
            }
        }

        // ── Prioridade 6: Logística (transporte de insumos) ──────────────
        {
            let logistics_intent = AgentIntent {
                kind: IntentKind::Transportar,
                target_agent: None,
                target_semantic: Some("logistica".to_string()),
                justification: "Motor autonomo: ajudando com logistica.".to_string(),
                dominant_emotion: "determinado".to_string(),
                perceived_risk: 0,
                belief_updates: Vec::new(),
                priority: 4,
                social_move: None,
            };
            self.bind_or_create_economic_task(agent_id, &logistics_intent)?;
            if let Some(task) = self.active_economic_task_for_agent(agent_id).cloned() {
                let intent = Self::intent_for_economic_task(&task);
                self.set_autopilot_intent(
                    agent_id,
                    &intent,
                    &format!("Logistica: {}", task.description),
                )?;
                return Ok(Some(intent));
            }
        }

        Ok(None)
    }

    fn set_autopilot_intent(
        &mut self,
        agent_id: u64,
        intent: &AgentIntent,
        thought: &str,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0 = Some(intent.clone());
        entity_mut
            .get_mut::<ThoughtComponent>()
            .ok_or_else(|| anyhow!("missing thought component"))?
            .0 = thought.to_string();
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
        Ok(())
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
            &LifeStatusComponent,
            &RelationComponent,
            &MemoryComponent,
            &PositionComponent,
            &DestinationLabelComponent,
            &IntentComponent,
            &DecisionBudgetComponent,
            &CognitionComponent,
            &ConversationComponent,
            &TaskQueueComponent,
            Option<&TraumaTrackerComponent>,
        )>();
        query
            .iter(&self.world)
            .map(
                |(
                    core,
                    profile,
                    state,
                    life_status,
                    relations,
                    memories,
                    position,
                    destination_label,
                    intent,
                    budget,
                    cognition,
                    conversation,
                    task_queue,
                    trauma_tracker,
                )| {
                    let tile = self.tile_at(position.0);
                    AgentContext {
                        id: core.id,
                        name: core.name.clone(),
                        role_id: core.role_id.clone(),
                        position: position.0,
                        state: state.0.clone(),
                        life_status: life_status.0,
                        profile: profile.0.clone(),
                        relations: relations.0.clone(),
                        memories: memories.0.clone(),
                        destination_label: destination_label.0.clone(),
                        current_building_id: tile.and_then(|entry| entry.building_id),
                        current_room_id: tile.and_then(|entry| entry.room_id),
                        last_intent: intent.0.clone(),
                        llm_calls: budget.llm_calls,
                        blocked_ticks: cognition.blocked_ticks,
                        active_conversation_id: conversation.active_conversation_id,
                        social_cooldown_until: conversation.social_cooldown_until,
                        household_id: core.home_building_id,
                        task_queue: task_queue.0.clone(),
                        trauma_tracker: trauma_tracker.map(|t| t.0.clone()).unwrap_or_default(),
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
            .role_id
            .clone();
        let role_def = self.role_def(&agent_role).cloned();
        let allowed_production_establishments = role_def
            .as_ref()
            .map(|def| {
                self.establishments
                    .iter()
                    .filter(|establishment| {
                        def.allowed_establishment_type_ids
                            .contains(&establishment.establishment_type_id)
                    })
                    .map(|establishment| establishment.id)
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
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
                .resource_id
                .as_ref()
                .map(|resource_id| target_hint.contains(resource_id))
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
            EconomicTaskKind::Produzir => task
                .related_establishment_id
                .map(|establishment_id| {
                    allowed_production_establishments.contains(&establishment_id)
                })
                .unwrap_or(false),
            EconomicTaskKind::ReceberPagamento => role_def
                .as_ref()
                .map(|def| def.can_collect_payments)
                .unwrap_or(false),
            EconomicTaskKind::Comprar
            | EconomicTaskKind::Transportar
            | EconomicTaskKind::Vender => role_def
                .as_ref()
                .map(|def| def.can_take_logistics_tasks)
                .unwrap_or(true),
        };

        let mut selected_task_id = self
            .economic_tasks
            .iter()
            .filter(|task| {
                task.actor_household_id == household_id
                    && task.kind == desired_kind
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
                    && (task.assigned_agent_id.is_none()
                        || task.assigned_agent_id == Some(agent_id))
                    && matches_target(task)
                    && role_allows_task(task)
                    && self.allow_food_support_assignment(household_id, agent_id, task)
            })
            .max_by(|a, b| {
                a.priority
                    .cmp(&b.priority)
                    .then_with(|| b.description.len().cmp(&a.description.len()))
            })
            .map(|task| task.id);

        if selected_task_id.is_none() {
            self.ensure_economic_tasks();
            selected_task_id = self
                .economic_tasks
                .iter()
                .filter(|task| {
                    task.actor_household_id == household_id
                        && task.kind == desired_kind
                        && task.phase != EconomicTaskPhase::Completed
                        && task.phase != EconomicTaskPhase::Failed
                        && (task.assigned_agent_id.is_none()
                            || task.assigned_agent_id == Some(agent_id))
                        && matches_target(task)
                        && role_allows_task(task)
                        && self.allow_food_support_assignment(household_id, agent_id, task)
                })
                .max_by(|a, b| {
                    a.priority
                        .cmp(&b.priority)
                        .then_with(|| b.description.len().cmp(&a.description.len()))
                })
                .map(|task| task.id);
        }

        if selected_task_id.is_none()
            && matches!(
                desired_kind,
                EconomicTaskKind::Comprar
                    | EconomicTaskKind::Transportar
                    | EconomicTaskKind::Produzir
            )
        {
            selected_task_id = self
                .economic_tasks
                .iter()
                .filter(|task| {
                    task.actor_household_id == household_id
                        && matches!(
                            task.class,
                            EconomicTaskClass::HouseholdFoodPurchase
                                | EconomicTaskClass::FoodSupplyTransport
                                | EconomicTaskClass::FoodProduction
                        )
                        && task.phase != EconomicTaskPhase::Completed
                        && task.phase != EconomicTaskPhase::Failed
                        && (task.assigned_agent_id.is_none()
                            || task.assigned_agent_id == Some(agent_id))
                        && role_allows_task(task)
                        && self.allow_food_support_assignment(household_id, agent_id, task)
                })
                .max_by(|a, b| a.priority.cmp(&b.priority))
                .map(|task| task.id);
        }

        if let Some(task_id) = selected_task_id {
            if let Some(task) = self
                .economic_tasks
                .iter_mut()
                .find(|task| task.id == task_id)
            {
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

    fn intent_for_economic_task(task: &EconomicTask) -> AgentIntent {
        let kind = match task.kind {
            EconomicTaskKind::Produzir => IntentKind::Trabalhar,
            EconomicTaskKind::Comprar => IntentKind::Comprar,
            EconomicTaskKind::Transportar => IntentKind::Transportar,
            EconomicTaskKind::Vender => IntentKind::Vender,
            EconomicTaskKind::ReceberPagamento => IntentKind::ReceberPagamento,
        };
        AgentIntent {
            kind,
            target_agent: None,
            target_semantic: Some(task.description.clone()),
            justification: format!("Concluir tarefa economica ativa: {}", task.description),
            dominant_emotion: "determinado".to_string(),
            perceived_risk: 2,
            belief_updates: Vec::new(),
            priority: task.priority.clamp(1, 10),
            social_move: None,
        }
    }

    fn has_extreme_incapacity(&self, context: &AgentContext) -> bool {
        context.state.energy <= 5 || context.state.health <= 10 || context.state.hunger >= 95
    }

    fn should_hold_locked_economic_task(&self, context: &AgentContext) -> bool {
        let Some(task) = self.active_economic_task_for_agent(context.id) else {
            return false;
        };
        task.lock_until_complete
            && task.phase != EconomicTaskPhase::Completed
            && task.phase != EconomicTaskPhase::Failed
            && context.blocked_ticks < BLOCKED_RECONSIDERATION_TICKS * 3
            && !self.has_extreme_incapacity(context)
    }

    fn fail_active_economic_task(
        &mut self,
        agent_id: u64,
        reason: &str,
        clear_intent: bool,
    ) -> Result<()> {
        let active_task = self.active_economic_task_for_agent(agent_id).cloned();
        let Some(task) = active_task else {
            return Ok(());
        };
        if let Some(task_state) = self
            .economic_tasks
            .iter_mut()
            .find(|entry| entry.id == task.id)
        {
            task_state.phase = EconomicTaskPhase::Failed;
            task_state.assigned_agent_id = None;
        }
        let entity = self.find_agent_entity(agent_id)?;
        let core_name = self
            .world
            .entity(entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .name
            .clone();
        self.world
            .entity_mut(entity)
            .get_mut::<EconomicActivityComponent>()
            .ok_or_else(|| anyhow!("missing economy component"))?
            .active_task_id = None;
        if clear_intent {
            self.clear_intent_navigation(agent_id)?;
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::Blocking,
            summary: format!(
                "{core_name} abandona a tarefa {}: {reason}.",
                task.description
            ),
            impact_tags: vec![
                "economia".to_string(),
                "falha".to_string(),
                "bloqueio".to_string(),
            ],
        });
        Ok(())
    }

    fn sync_intent_with_locked_task(&mut self, agent_id: u64) -> Result<Option<AgentIntent>> {
        let active_task = self.active_economic_task_for_agent(agent_id).cloned();
        let Some(task) = active_task else {
            return Ok(None);
        };
        if !task.lock_until_complete {
            return Ok(None);
        }
        let expected_intent = Self::intent_for_economic_task(&task);
        let entity = self.find_agent_entity(agent_id)?;
        let current_intent = self
            .world
            .entity(entity)
            .get::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0
            .clone();
        let needs_sync = current_intent
            .as_ref()
            .map(|intent| {
                intent.kind != expected_intent.kind
                    || intent.target_semantic != expected_intent.target_semantic
            })
            .unwrap_or(true);
        if needs_sync {
            let mut entity_mut = self.world.entity_mut(entity);
            entity_mut
                .get_mut::<IntentComponent>()
                .ok_or_else(|| anyhow!("missing intent component"))?
                .0 = Some(expected_intent.clone());
            entity_mut
                .get_mut::<ThoughtComponent>()
                .ok_or_else(|| anyhow!("missing thought component"))?
                .0 = format!("Persistindo tarefa economica ativa: {}", task.description);
            entity_mut
                .get_mut::<DestinationLabelComponent>()
                .ok_or_else(|| anyhow!("missing destination label component"))?
                .0 = expected_intent.target_semantic.clone();
        }
        Ok(Some(expected_intent))
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
            && let Some(task) = self
                .economic_tasks
                .iter_mut()
                .find(|task| task.id == task_id)
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
        let _ = self.sync_intent_with_locked_task(agent_id)?;
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
        if intent.kind == IntentKind::Comer && !self.household_has_food_available(agent_id)? {
            if self.reroute_eat_intent_to_food_purchase(agent_id)? {
                return self.ensure_navigation_for_current_intent(agent_id);
            }
        }
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
            if self
                .active_economic_task_for_agent(agent_id)
                .map(|task| task.lock_until_complete)
                .unwrap_or(false)
            {
                self.increment_blocked_ticks(agent_id)?;
                if self.blocked_ticks(agent_id)? >= BLOCKED_RECONSIDERATION_TICKS * 3 {
                    self.fail_active_economic_task(
                        agent_id,
                        "nao encontrou rota ou alvo economico valido por tempo demais",
                        true,
                    )?;
                }
                return Ok(());
            }
            if self.try_rebind_household_food_intent(agent_id)? {
                return self.ensure_navigation_for_current_intent(agent_id);
            }
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
            self.increment_blocked_ticks(agent_id)?;
            if self
                .active_economic_task_for_agent(agent_id)
                .map(|task| task.lock_until_complete)
                .unwrap_or(false)
                && self.blocked_ticks(agent_id)? >= BLOCKED_RECONSIDERATION_TICKS * 3
            {
                self.fail_active_economic_task(
                    agent_id,
                    "permaneceu bloqueado no caminho da tarefa economica",
                    true,
                )?;
                return Ok(());
            }
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
        if self.apply_emergency_food_rule(agent_id)? {
            return Ok(());
        }
        let synced_intent = self.sync_intent_with_locked_task(agent_id)?;
        let mut intent = self
            .world
            .entity(entity)
            .get::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0
            .clone();
        if synced_intent.is_some() {
            intent = synced_intent;
        }

        if intent.is_none() {
            if let Some(task) = self.active_economic_task_for_agent(agent_id).cloned() {
                let restored = Self::intent_for_economic_task(&task);
                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<IntentComponent>()
                    .ok_or_else(|| anyhow!("missing intent component"))?
                    .0 = Some(restored.clone());
                entity_mut
                    .get_mut::<ThoughtComponent>()
                    .ok_or_else(|| anyhow!("missing thought component"))?
                    .0 = format!("Persistindo tarefa economica ativa: {}", task.description);
                entity_mut
                    .get_mut::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0 = None;
                entity_mut
                    .get_mut::<DestinationLabelComponent>()
                    .ok_or_else(|| anyhow!("missing destination label component"))?
                    .0 = restored.target_semantic.clone();
                entity_mut
                    .get_mut::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0
                    .clear();
                drop(entity_mut);
                self.ensure_navigation_for_current_intent(agent_id)?;
                intent = Some(restored);
            }
        }

        if intent.is_none() {
            let task_opt = {
                let mut entity_mut = self.world.entity_mut(entity);
                let mut queue = entity_mut
                    .get_mut::<TaskQueueComponent>()
                    .ok_or_else(|| anyhow!("missing task queue component"))?;
                queue.0.pop_front()
            };

            if let Some(task) = task_opt {
                let current_pos = self
                    .world
                    .entity(entity)
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0;
                let mut nearby_ids = Vec::new();
                let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
                for (core, position) in query.iter(&self.world) {
                    if core.id != agent_id && current_pos.manhattan(position.0) <= 6 {
                        nearby_ids.push(core.id);
                    }
                }

                let new_intent = AgentIntent {
                    kind: task.kind,
                    target_agent: task.target_agent,
                    target_semantic: task.target_semantic.clone(),
                    justification: format!("Executando tarefa da fila: {:?}", task.kind),
                    dominant_emotion: "determinado".to_string(),
                    perceived_risk: 0,
                    belief_updates: Vec::new(),
                    priority: 1,
                    social_move: task.social_move,
                };
                let validated = validate_intent(new_intent, &nearby_ids);

                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<IntentComponent>()
                    .ok_or_else(|| anyhow!("missing intent component"))?
                    .0 = Some(validated.clone());
                entity_mut
                    .get_mut::<ThoughtComponent>()
                    .ok_or_else(|| anyhow!("missing thought component"))?
                    .0 = format!("Sequencia: {:?}", validated.kind);
                entity_mut
                    .get_mut::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0 = None;
                entity_mut
                    .get_mut::<DestinationLabelComponent>()
                    .ok_or_else(|| anyhow!("missing destination label component"))?
                    .0 = validated.target_semantic.clone();
                entity_mut
                    .get_mut::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0
                    .clear();

                drop(entity_mut);

                if matches!(
                    validated.kind,
                    IntentKind::Comprar
                        | IntentKind::Transportar
                        | IntentKind::Vender
                        | IntentKind::ReceberPagamento
                        | IntentKind::Trabalhar
                ) {
                    self.bind_or_create_economic_task(agent_id, &validated)?;
                } else {
                    self.clear_active_economic_task(agent_id)?;
                }

                self.ensure_navigation_for_current_intent(agent_id)?;

                intent = Some(validated);
            }
        }

        // ── Motor Econômico Autônomo ──────────────────────────────────────
        // Se o agente ainda não tem intent (aguardando LLM ou recém-criado),
        // aplica regras determinísticas de sobrevivência e produção.
        if intent.is_none() {
            intent = self.apply_survival_economy(agent_id)?;
            if intent.is_some() {
                self.ensure_navigation_for_current_intent(agent_id)?;
            }
        }

        let Some(intent) = intent else {
            return Ok(());
        };
        if intent.kind == IntentKind::Comer && !self.household_has_food_available(agent_id)? {
            if self.reroute_eat_intent_to_food_purchase(agent_id)? {
                return self.try_execute_current_intent(agent_id, llm);
            }
        }
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
            IntentKind::Agredir => self.apply_assault_intent(agent_id, intent.target_agent)?,
            IntentKind::Combater => self.apply_combat_intent(agent_id, intent.target_agent)?,
            IntentKind::Roubar => self.apply_robbery_intent(agent_id, intent.target_agent)?,
            IntentKind::Furtar => self.apply_theft_intent(agent_id, intent.target_agent)?,
            IntentKind::Fugir => self.apply_flee_intent(agent_id)?,
            IntentKind::Acusar => self.apply_accuse_intent(agent_id, intent.target_agent)?,
            IntentKind::Investigar => self.apply_investigate_intent(agent_id)?,
            IntentKind::Prender => self.apply_arrest_intent(agent_id, intent.target_agent)?,
            IntentKind::Punir => self.apply_punish_intent(agent_id, intent.target_agent)?,
            IntentKind::Apoiar => self.apply_political_support_intent(agent_id, true)?,
            IntentKind::Opor => self.apply_political_support_intent(agent_id, false)?,
            IntentKind::Pressionar => {
                self.apply_political_pressure_intent(agent_id, intent.target_agent)?
            }
            IntentKind::PedirApoio => {
                self.apply_political_request_support_intent(agent_id, intent.target_agent)?
            }
            IntentKind::Mediar => {
                self.apply_political_mediate_intent(agent_id, intent.target_agent)?
            }
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
            IntentKind::Trabalhar
            | IntentKind::Comprar
            | IntentKind::Transportar
            | IntentKind::Vender
            | IntentKind::ReceberPagamento => {
                let task_opt = self.active_economic_task_for_agent(agent_id).cloned();
                if let Some(task) = task_opt {
                    if task.phase == EconomicTaskPhase::Completed {
                        self.clear_intent_navigation(agent_id)?;
                        self.clear_active_economic_task(agent_id)?;
                    } else if task.phase == EconomicTaskPhase::Failed {
                        let entity = self.find_agent_entity(agent_id)?;
                        self.world
                            .entity_mut(entity)
                            .get_mut::<TaskQueueComponent>()
                            .ok_or_else(|| anyhow!("missing task queue component"))?
                            .0
                            .clear();
                        self.clear_intent_navigation(agent_id)?;
                        self.clear_active_economic_task(agent_id)?;
                        self.add_memory(
                            agent_id,
                            MemoryKind::Failure,
                            format!("Tarefa falhou: {}", task.description),
                            vec!["falha".to_string(), "economia".to_string()],
                            10,
                            Vec::new(),
                        )?;
                    }
                } else {
                    self.clear_intent_navigation(agent_id)?;
                }
            }
            IntentKind::Agredir
            | IntentKind::Combater
            | IntentKind::Roubar
            | IntentKind::Furtar
            | IntentKind::Fugir
            | IntentKind::Acusar
            | IntentKind::Investigar
            | IntentKind::Prender
            | IntentKind::Punir
            | IntentKind::Apoiar
            | IntentKind::Opor
            | IntentKind::Pressionar
            | IntentKind::PedirApoio
            | IntentKind::Mediar => self.clear_intent_navigation(agent_id)?,
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
        let (current_pos, destination) = {
            let entry = self.world.entity(entity);
            (
                entry
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0,
                entry
                    .get::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0,
            )
        };
        match intent.kind {
            IntentKind::Comer => {
                if self.household_has_food_available(agent_id)? {
                    return Ok(true);
                }
                Ok(destination
                    .map(|target| target == current_pos)
                    .unwrap_or(false))
            }
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
            | IntentKind::ReceberPagamento => Ok(destination
                .map(|destination| destination == current_pos)
                .unwrap_or(false)),
            IntentKind::Agredir
            | IntentKind::Combater
            | IntentKind::Roubar
            | IntentKind::Prender
            | IntentKind::Punir
            | IntentKind::Pressionar
            | IntentKind::PedirApoio
            | IntentKind::Mediar => {
                if let Some(target_id) = intent.target_agent {
                    self.agents_adjacent(agent_id, target_id)
                } else {
                    Ok(false)
                }
            }
            IntentKind::Furtar => {
                if let Some(target_id) = intent.target_agent {
                    Ok(self
                        .agent_distance_from(current_pos, target_id)
                        .is_some_and(|distance| distance <= 2))
                } else {
                    Ok(false)
                }
            }
            IntentKind::Fugir
            | IntentKind::Acusar
            | IntentKind::Investigar
            | IntentKind::Apoiar
            | IntentKind::Opor => Ok(true),
            _ => Ok(destination
                .map(|destination| destination == current_pos)
                .unwrap_or(false)),
        }
    }

    fn household_has_food_available(&mut self, agent_id: u64) -> Result<bool> {
        let Some(household_id) = self.household_id_for_agent(agent_id) else {
            return Ok(false);
        };
        Ok(self.household_has_ready_food_available(household_id)
            || self.household_has_reserved_food_available(household_id))
    }

    fn reroute_eat_intent_to_food_purchase(&mut self, agent_id: u64) -> Result<bool> {
        if self.household_has_food_available(agent_id)? {
            return Ok(false);
        }
        let Some(household_id) = self.household_id_for_agent(agent_id) else {
            return Ok(false);
        };
        self.ensure_economic_tasks();
        let purchase_hint = self
            .best_food_source_for_household(household_id)
            .map(|(_, resource, _)| format!("comida {}", resource.as_str()))
            .unwrap_or_else(|| "comida para a despensa".to_string());
        let purchase_intent = AgentIntent {
            kind: IntentKind::Comprar,
            target_agent: None,
            target_semantic: Some(purchase_hint.clone()),
            justification:
                "A despensa do lar esta vazia; primeiro preciso comprar comida para poder comer."
                    .to_string(),
            dominant_emotion: "urgencia".to_string(),
            perceived_risk: 8,
            belief_updates: vec![
                "Sem repor alimento, a fome vai piorar imediatamente.".to_string(),
            ],
            priority: 10,
            social_move: None,
        };

        self.bind_or_create_economic_task(agent_id, &purchase_intent)?;
        let task_found = self
            .active_economic_task_for_agent(agent_id)
            .map(|task| task.kind == EconomicTaskKind::Comprar)
            .unwrap_or(false);
        let agent_name = self.agent_name(agent_id)?;

        if task_found {
            let entity = self.find_agent_entity(agent_id)?;
            {
                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<IntentComponent>()
                    .ok_or_else(|| anyhow!("missing intent component"))?
                    .0 = Some(purchase_intent.clone());
                entity_mut
                    .get_mut::<ThoughtComponent>()
                    .ok_or_else(|| anyhow!("missing thought component"))?
                    .0 = "Despensa vazia: redirecionando para comprar alimento.".to_string();
                entity_mut
                    .get_mut::<DestinationComponent>()
                    .ok_or_else(|| anyhow!("missing destination component"))?
                    .0 = None;
                entity_mut
                    .get_mut::<DestinationLabelComponent>()
                    .ok_or_else(|| anyhow!("missing destination label component"))?
                    .0 = purchase_intent.target_semantic.clone();
                entity_mut
                    .get_mut::<PathComponent>()
                    .ok_or_else(|| anyhow!("missing path component"))?
                    .0
                    .clear();
            }

            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Commerce,
                summary: format!(
                    "{agent_name} encontra a despensa vazia e muda de comer para comprar alimento."
                ),
                impact_tags: vec![
                    "fome".to_string(),
                    "compra".to_string(),
                    "despensa".to_string(),
                ],
            });
        } else {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Scarcity,
                summary: format!(
                    "{agent_name} tenta trocar comer por compra de alimento, mas nao encontra oferta viavel."
                ),
                impact_tags: vec!["fome".to_string(), "compra".to_string(), "despensa".to_string()],
            });
        }
        Ok(task_found)
    }

    fn try_rebind_household_food_intent(&mut self, agent_id: u64) -> Result<bool> {
        let Some(household_id) = self.household_id_for_agent(agent_id) else {
            return Ok(false);
        };
        let Some(household) = self.household_by_id(household_id) else {
            return Ok(false);
        };
        if household.food_crisis_level == 0 {
            return Ok(false);
        }
        self.ensure_economic_tasks();
        let fallback_intent = AgentIntent {
            kind: IntentKind::Comprar,
            target_agent: None,
            target_semantic: Some("comida para a despensa".to_string()),
            justification: "O lar segue em crise alimentar; preciso assumir a melhor tarefa de abastecimento disponivel.".to_string(),
            dominant_emotion: "urgencia".to_string(),
            perceived_risk: 6,
            belief_updates: vec!["Abastecimento de comida tem prioridade sobre a rotina agora.".to_string()],
            priority: 9,
            social_move: None,
        };
        self.bind_or_create_economic_task(agent_id, &fallback_intent)?;
        let task_found = self
            .active_economic_task_for_agent(agent_id)
            .map(|task| task.kind == EconomicTaskKind::Comprar)
            .unwrap_or(false);
        if task_found {
            let agent_name = self.agent_name(agent_id)?;
            let entity = self.find_agent_entity(agent_id)?;
            let mut entity_mut = self.world.entity_mut(entity);
            entity_mut
                .get_mut::<IntentComponent>()
                .ok_or_else(|| anyhow!("missing intent component"))?
                .0 = Some(fallback_intent);
            entity_mut
                .get_mut::<ThoughtComponent>()
                .ok_or_else(|| anyhow!("missing thought component"))?
                .0 = "Rebinding para tarefa alimentar prioritaria do lar.".to_string();
            entity_mut
                .get_mut::<DestinationComponent>()
                .ok_or_else(|| anyhow!("missing destination component"))?
                .0 = None;
            entity_mut
                .get_mut::<DestinationLabelComponent>()
                .ok_or_else(|| anyhow!("missing destination label component"))?
                .0 = Some("abastecimento alimentar".to_string());
            entity_mut
                .get_mut::<PathComponent>()
                .ok_or_else(|| anyhow!("missing path component"))?
                .0
                .clear();
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: None,
                kind: EventKind::Commerce,
                summary: format!("{agent_name} e desviado para o abastecimento alimentar do lar."),
                impact_tags: vec!["abastecimento".to_string(), "desvio".to_string()],
            });
        }
        Ok(task_found)
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
        if !self.is_walkable(next_step) {
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
            IntentKind::Agredir
            | IntentKind::Combater
            | IntentKind::Roubar
            | IntentKind::Furtar
            | IntentKind::Prender
            | IntentKind::Punir
            | IntentKind::Pressionar
            | IntentKind::PedirApoio
            | IntentKind::Mediar => self.social_candidates(core.id, intent.target_agent),
            IntentKind::Fugir
            | IntentKind::Acusar
            | IntentKind::Investigar
            | IntentKind::Apoiar
            | IntentKind::Opor => {
                vec![ResolvedTargetCandidate {
                    destination: current_pos,
                    label: intent
                        .target_semantic
                        .clone()
                        .unwrap_or_else(|| intent.kind.as_str().to_string()),
                }]
            }
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
        if core.role_id == Role::Farmer.id() {
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
            if self.is_walkable(neighbor) {
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
                .or_else(|| {
                    self.building_by_id(*building_id)
                        .map(|building| building.entrance)
                }),
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
        resource_id: &str,
        amount: i32,
    ) -> i32 {
        match node {
            EconomicNode::HouseholdPantry(building_id) => self
                .household_by_id_mut(*building_id)
                .map(|household| Self::take_resource(&mut household.pantry, resource_id, amount))
                .unwrap_or(0),
            EconomicNode::Establishment(establishment_id) => self
                .establishment_by_id_mut(*establishment_id)
                .map(|establishment| {
                    Self::take_resource(&mut establishment.stock, resource_id, amount)
                })
                .unwrap_or(0),
            EconomicNode::ExternalMarket => amount.max(0),
            EconomicNode::PublicTreasury => 0,
        }
    }

    fn add_resource_to_node(&mut self, node: &EconomicNode, resource_id: &str, amount: i32) {
        match node {
            EconomicNode::HouseholdPantry(building_id) => {
                if let Some(household) = self.household_by_id_mut(*building_id) {
                    Self::push_resource(&mut household.pantry, resource_id, amount);
                }
            }
            EconomicNode::Establishment(establishment_id) => {
                if let Some(establishment) = self.establishment_by_id_mut(*establishment_id) {
                    Self::push_resource(&mut establishment.stock, resource_id, amount);
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
        if let Some(task_state) = self
            .economic_tasks
            .iter_mut()
            .find(|entry| entry.id == task.id)
        {
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
        let resource_id = task
            .resource_id
            .clone()
            .ok_or_else(|| anyhow!("economic task {} missing resource", task.id))?;
        match task.phase {
            EconomicTaskPhase::AwaitingPickup => {
                let agent_name = self.agent_name(agent_id)?;
                if task.kind == EconomicTaskKind::Comprar && !self.withdraw_cash_for_purchase(&task)
                {
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: agent_id,
                        target: None,
                        kind: EventKind::Scarcity,
                        summary: format!(
                            "{agent_name} nao tem caixa suficiente para {}.",
                            task.description
                        ),
                        impact_tags: vec!["escassez".to_string(), "caixa".to_string()],
                    });
                    if let Some(task_state) = self
                        .economic_tasks
                        .iter_mut()
                        .find(|entry| entry.id == task.id)
                    {
                        task_state.phase = EconomicTaskPhase::Failed;
                    }
                    self.clear_active_economic_task(agent_id)?;
                    return Ok(());
                }
                let agent_entity = self.find_agent_entity(agent_id)?;
                let carrying_capacity = self
                    .world
                    .entity(agent_entity)
                    .get::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?
                    .carrying_capacity
                    .max(1);
                let pickup_amount = task.amount.min(carrying_capacity);
                let amount =
                    self.remove_resource_from_node(&task.source, &resource_id, pickup_amount);
                if amount <= 0 {
                    if let Some(task_state) = self
                        .economic_tasks
                        .iter_mut()
                        .find(|entry| entry.id == task.id)
                    {
                        task_state.phase = EconomicTaskPhase::Failed;
                    }
                    self.clear_active_economic_task(agent_id)?;
                    return Ok(());
                }
                let entity = self.find_agent_entity(agent_id)?;
                if task.creates_household_reserve
                    && matches!(task.destination, EconomicNode::HouseholdPantry(_))
                {
                    self.world
                        .entity_mut(entity)
                        .get_mut::<EconomicActivityComponent>()
                        .ok_or_else(|| anyhow!("missing economy component"))?
                        .carrying
                        .clear();
                    if let Some(household) = self.household_by_id_mut(task.actor_household_id) {
                        Self::push_resource(&mut household.reserved_food, &resource_id, amount);
                    }
                } else {
                    self.world
                        .entity_mut(entity)
                        .get_mut::<EconomicActivityComponent>()
                        .ok_or_else(|| anyhow!("missing economy component"))?
                        .carrying = vec![ResourceStack {
                        resource_id: resource_id.clone(),
                        amount,
                    }];
                }
                if let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
                {
                    task_state.phase = EconomicTaskPhase::InTransit;
                    task_state.amount = amount;
                    task_state.total_price = task_state.unit_price * amount;
                }
                self.deposit_cash_to_sale_target(&task);
                self.sync_establishment_stocks_to_fixtures();
            }
            EconomicTaskPhase::InTransit => {
                let agent_name = self.agent_name(agent_id)?;
                let delivered_amount = if task.creates_household_reserve
                    && matches!(task.destination, EconomicNode::HouseholdPantry(_))
                {
                    if let Some(household) = self.household_by_id_mut(task.actor_household_id) {
                        Self::take_resource(&mut household.reserved_food, &resource_id, task.amount)
                    } else {
                        0
                    }
                } else {
                    let entity = self.find_agent_entity(agent_id)?;
                    let entry = self.world.entity(entity);
                    entry
                        .get::<EconomicActivityComponent>()
                        .ok_or_else(|| anyhow!("missing economy component"))?
                        .carrying
                        .iter()
                        .find(|stack| stack.resource_id == resource_id)
                        .map(|stack| stack.amount)
                        .unwrap_or(0)
                };
                if delivered_amount > 0 {
                    self.add_resource_to_node(&task.destination, &resource_id, delivered_amount);
                }
                let entity = self.find_agent_entity(agent_id)?;
                let mut entity_mut = self.world.entity_mut(entity);
                let mut economic = entity_mut
                    .get_mut::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?;
                economic.carrying.clear();
                economic.active_task_id = None;
                if let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
                {
                    task_state.phase = EconomicTaskPhase::Completed;
                }
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: None,
                    kind: EventKind::Logistics,
                    summary: format!("{agent_name} conclui a tarefa: {}.", task.description),
                    impact_tags: vec!["logistica".to_string(), resource_id.clone()],
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
                    if let Some(existing) = household.pending_payments.iter_mut().find(|pending| {
                        pending.payer_label == claim.payer_label && pending.amount == claim.amount
                    }) {
                        existing.amount -= paid;
                    }
                    household
                        .pending_payments
                        .retain(|pending| pending.amount > 0);
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
        let intent_opt = self
            .world
            .entity(entity)
            .get::<IntentComponent>()
            .map(|ic| ic.0.clone())
            .flatten();
        if let Some(ref intent) = intent_opt {
            if intent.target_semantic.as_deref() == Some("motim_comida") {
                return self.execute_food_riot_steal(actor_id);
            }
        }

        let active_production_task = self
            .active_economic_task_for_agent(actor_id)
            .filter(|task| task.kind == EconomicTaskKind::Produzir)
            .cloned();
        let (name, role_id, work_building_id, home_building_id) = {
            let entry = self.world.entity(entity);
            let core = entry
                .get::<AgentCore>()
                .ok_or_else(|| anyhow!("missing agent core"))?;
            (
                core.name.clone(),
                core.role_id.clone(),
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
            state.0.energy = (state.0.energy - 7).clamp(0, 100);
            state.0.hunger = (state.0.hunger + 6).clamp(0, 100);
            state.0.stress = (state.0.stress + 2).clamp(0, 100);
            state.0.mood = (state.0.mood + 1).clamp(0, 100);
        }
        let mut produced = ResourceStack {
            resource_id: ResourceKind::Moedas.id().to_string(),
            amount: 0,
        };
        let mut work_failed_reason = None::<String>;
        let mut salary_claim = None::<PendingPaymentClaim>;
        let role_name = self.role_display_name(&role_id);
        if let Some(building_id) = work_building_id {
            let est_info = self.establishment_by_building(building_id).map(|est| {
                (
                    est.id,
                    est.name.clone(),
                    est.establishment_type_id.clone(),
                    est.public_service,
                    est.stock.clone(),
                )
            });

            if let Some((_est_id, est_name, est_type, est_public_service, est_stock)) = est_info {
                let recipe = self.recipe_for_establishment_type(&est_type).cloned();

                enum FarmAction {
                    Harvest(Vec<TileCoord>),
                    Plant(Vec<TileCoord>),
                    FailGrowing,
                }

                let farm_action = if est_type == "fazenda" {
                    let farm_buildings: Vec<&BuildingSpec> = self.spatial.buildings.iter()
                        .filter(|b| b.kind == LocationKind::Farm)
                        .collect();
                    let farm_fields: Vec<TileCoord> = self.spatial.grid.tiles.iter()
                        .filter(|tile| tile.kind == TileKind::Field)
                        .map(|tile| tile.coord)
                        .filter(|&coord| {
                            if farm_buildings.is_empty() {
                                false
                            } else {
                                let closest = farm_buildings.iter()
                                    .min_by_key(|b| b.entrance.manhattan(coord))
                                    .unwrap();
                                closest.id == building_id
                            }
                        })
                        .collect();

                    let mut ready_fields = Vec::new();
                    let mut empty_fields = Vec::new();
                    for coord in farm_fields {
                        if let Some(crop) = self.crops.get(&coord) {
                            if crop.stage == CropStage::Ready {
                                ready_fields.push(coord);
                            }
                        } else {
                            empty_fields.push(coord);
                        }
                    }

                    if !ready_fields.is_empty() {
                        FarmAction::Harvest(ready_fields)
                    } else if !empty_fields.is_empty() {
                        FarmAction::Plant(empty_fields)
                    } else {
                        FarmAction::FailGrowing
                    }
                } else {
                    FarmAction::FailGrowing
                };

                let mut can_work = false;

                if est_type == "fazenda" {
                    match farm_action {
                        FarmAction::Harvest(ready_fields) => {
                            let mut has_tools = true;
                            if let Some(ref recipe) = recipe {
                                let missing_capital = recipe.capital_requirements.iter().find(|requirement| {
                                    Self::total_resource_amount(&est_stock, &requirement.resource_id)
                                        < requirement.amount
                                });
                                if let Some(requirement) = missing_capital {
                                    work_failed_reason = Some(format!(
                                        "faltam {} em {}",
                                        requirement.resource_id, est_name
                                    ));
                                    has_tools = false;
                                }
                            }
                            if has_tools {
                                for coord in ready_fields {
                                    self.crops.remove(&coord);
                                }
                                produced = ResourceStack {
                                    resource_id: ResourceKind::Graos.id().to_string(),
                                    amount: recipe.as_ref().map(|r| r.output_amount).unwrap_or(6),
                                };
                                if let Some(establishment) = self.establishment_by_building_mut(building_id) {
                                    if let Some(ref recipe) = recipe {
                                        if recipe.tool_wear > 0 && !recipe.capital_requirements.is_empty() {
                                            establishment.tool_wear += recipe.tool_wear;
                                            while establishment.tool_wear >= 4 {
                                                let mut degraded = false;
                                                for capital in &recipe.capital_requirements {
                                                    let removed = Self::take_resource(
                                                        &mut establishment.stock,
                                                        &capital.resource_id,
                                                        1,
                                                    );
                                                    if removed > 0 {
                                                        degraded = true;
                                                    }
                                                }
                                                establishment.tool_wear -= 4;
                                                if !degraded {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                can_work = true;
                            }
                        }
                        FarmAction::Plant(empty_fields) => {
                            for coord in empty_fields {
                                self.crops.insert(coord, CropState {
                                    stage: CropStage::Planted,
                                    ticks_since_planted: 0,
                                });
                            }
                            produced = ResourceStack {
                                resource_id: ResourceKind::Graos.id().to_string(),
                                amount: 0,
                            };
                            can_work = true;
                        }
                        FarmAction::FailGrowing => {
                            work_failed_reason = Some("plantacoes ainda crescendo".to_string());
                            can_work = false;
                        }
                    }
                } else {
                    if let Some(ref recipe) = recipe {
                        let missing_capital = recipe.capital_requirements.iter().find(|requirement| {
                            Self::total_resource_amount(&est_stock, &requirement.resource_id)
                                < requirement.amount
                        });
                        if let Some(requirement) = missing_capital {
                            work_failed_reason = Some(format!(
                                "faltam {} em {}",
                                requirement.resource_id, est_name
                            ));
                            can_work = false;
                        } else {
                            if let Some(establishment) = self.establishment_by_building_mut(building_id) {
                                let mut consumed_inputs = Vec::new();
                                let mut enough_inputs = true;
                                for input in &recipe.inputs {
                                    let taken = Self::take_resource(
                                        &mut establishment.stock,
                                        &input.resource_id,
                                        input.amount,
                                    );
                                    if taken < input.amount {
                                        consumed_inputs.push((input.resource_id.clone(), taken));
                                        enough_inputs = false;
                                        break;
                                    }
                                    consumed_inputs.push((input.resource_id.clone(), taken));
                                }
                                if !enough_inputs {
                                    for (resource_id, amount) in consumed_inputs {
                                        if amount > 0 {
                                            Self::push_resource(
                                                &mut establishment.stock,
                                                &resource_id,
                                                amount,
                                            );
                                        }
                                    }
                                    work_failed_reason = Some(format!(
                                        "faltam insumos para {} em {}",
                                        recipe.output_resource_id, establishment.name
                                    ));
                                    can_work = false;
                                } else {
                                    produced = ResourceStack {
                                        resource_id: recipe.output_resource_id.clone(),
                                        amount: recipe.output_amount,
                                    };
                                    if recipe.tool_wear > 0 && !recipe.capital_requirements.is_empty() {
                                        establishment.tool_wear += recipe.tool_wear;
                                        while establishment.tool_wear >= 4 {
                                            let mut degraded = false;
                                            for capital in &recipe.capital_requirements {
                                                let removed = Self::take_resource(
                                                    &mut establishment.stock,
                                                    &capital.resource_id,
                                                    1,
                                                );
                                                if removed > 0 {
                                                    degraded = true;
                                                }
                                            }
                                            establishment.tool_wear -= 4;
                                            if !degraded {
                                                break;
                                            }
                                        }
                                    }
                                    can_work = true;
                                }
                            }
                        }
                    } else {
                        can_work = est_public_service;
                    }
                }

                if can_work {
                    if let Some(establishment) = self.establishment_by_building_mut(building_id) {
                        if produced.amount > 0 {
                            Self::push_resource(
                                &mut establishment.stock,
                                &produced.resource_id,
                                produced.amount,
                            );
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
                format!("{name} tenta trabalhar como {}, mas {}.", role_name, reason)
            } else {
                format!("{name} trabalha como {}.", role_name)
            },
            impact_tags: vec!["trabalho".to_string(), produced.resource_id.clone()],
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
                    format!(
                        "Trabalho concluido produzindo {}.",
                        self.resource_display_name(&produced.resource_id)
                    )
                } else {
                    "Trabalho civico concluido e pagamento aguardado.".to_string()
                },
                vec!["trabalho".to_string(), produced.resource_id.clone()],
                8,
                Vec::new(),
            )?;
        } else {
            if let Some(task) = active_production_task
                && let Some(task_state) = self
                    .economic_tasks
                    .iter_mut()
                    .find(|entry| entry.id == task.id)
            {
                task_state.phase = EconomicTaskPhase::Failed;
            }
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
            state.0.energy = (state.0.energy + 22).clamp(0, 100);
            state.0.stress = (state.0.stress - 12).clamp(0, 100);
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
                state.0.hunger = (state.0.hunger - 38).clamp(0, 100);
                state.0.energy = (state.0.energy + 4).clamp(0, 100);
                state.0.stress = (state.0.stress - 6).clamp(0, 100);
                state.0.mood = (state.0.mood + 5).clamp(0, 100);
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

    fn apply_assault_intent(&mut self, actor_id: u64, target_id: Option<u64>) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        self.apply_attack(actor_id, target_id, false)
    }

    fn apply_combat_intent(&mut self, actor_id: u64, target_id: Option<u64>) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        self.apply_attack(actor_id, target_id, true)
    }

    fn apply_attack(
        &mut self,
        actor_id: u64,
        target_id: u64,
        continuing_combat: bool,
    ) -> Result<()> {
        if !self.can_agent_act(actor_id)? || !self.can_receive_violence(target_id)? {
            return Ok(());
        }
        if !self.agents_adjacent(actor_id, target_id)? {
            let actor_name = self.agent_name(actor_id)?;
            let target_name = self.agent_name(target_id)?;
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: actor_id,
                target: Some(target_id),
                kind: EventKind::Blocking,
                summary: format!(
                    "{actor_name} tenta agredir {target_name}, mas nao esta adjacente."
                ),
                impact_tags: vec!["violencia".to_string(), "distancia".to_string()],
            });
            return Ok(());
        }

        self.interrupt_agent_conversations(actor_id, ConversationOutcome::PhysicalConflict)?;
        self.interrupt_agent_conversations(target_id, ConversationOutcome::PhysicalConflict)?;
        self.clear_active_economic_task(actor_id)?;
        self.clear_active_economic_task(target_id)?;

        let actor_state = self.agent_state(actor_id)?;
        let target_life_before = self.life_status(target_id)?;
        let base_damage = if continuing_combat { 9 } else { 12 };
        let energy_bonus = if actor_state.energy >= 50 { 4 } else { 0 };
        let vulnerability_bonus = if target_life_before == AgentLifeStatus::Incapacitado {
            8
        } else {
            0
        };
        let damage = base_damage + energy_bonus + vulnerability_bonus;
        let mut target_died = false;
        let mut target_incapacitated = false;

        {
            let target_entity = self.find_agent_entity(target_id)?;
            let mut entity_mut = self.world.entity_mut(target_entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.health = (state.0.health - damage).clamp(0, 100);
            state.0.stress = (state.0.stress + 18).clamp(0, 100);
            state.0.mood = (state.0.mood - 14).clamp(0, 100);
            let remaining_health = state.0.health;
            drop(state);
            let mut injury = entity_mut
                .get_mut::<InjuryComponent>()
                .ok_or_else(|| anyhow!("missing injury component"))?;
            injury.0.pain = (injury.0.pain + damage).clamp(0, 100);
            if damage >= 16 {
                injury.0.severe_wounds = injury.0.severe_wounds.saturating_add(1);
                injury.0.bleeding = (injury.0.bleeding + 2).clamp(0, 10);
            } else {
                injury.0.light_wounds = injury.0.light_wounds.saturating_add(1);
                injury.0.bleeding = (injury.0.bleeding + 1).clamp(0, 10);
            }
            injury.0.recovery_ticks = injury.0.recovery_ticks.max(30);
            drop(injury);
            if remaining_health <= 0 {
                target_died = true;
            } else if remaining_health <= 15 {
                target_incapacitated = true;
                entity_mut
                    .get_mut::<LifeStatusComponent>()
                    .ok_or_else(|| anyhow!("missing life status component"))?
                    .0 = AgentLifeStatus::Incapacitado;
            }
        }

        {
            let actor_entity = self.find_agent_entity(actor_id)?;
            let mut entity_mut = self.world.entity_mut(actor_entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.energy = (state.0.energy - 10).clamp(0, 100);
            state.0.stress = (state.0.stress + 12).clamp(0, 100);
        }

        let actor_name = self.agent_name(actor_id)?;
        let target_name = self.agent_name(target_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: Some(target_id),
            kind: if target_died {
                EventKind::Death
            } else {
                EventKind::Violence
            },
            summary: if target_died {
                format!("{actor_name} fere mortalmente {target_name}.")
            } else if target_incapacitated {
                format!("{actor_name} agride {target_name} e o deixa incapacitado.")
            } else {
                format!("{actor_name} agride {target_name}, causando {damage} de dano.")
            },
            impact_tags: vec!["violencia".to_string(), "crime".to_string()],
        });
        self.apply_relation_delta(
            target_id,
            actor_id,
            &RelationDelta {
                trust: -22,
                friendship: -18,
                resentment: 35,
                attraction: 0,
                moral_debt: 0,
                reputation: -10,
            },
        )?;
        self.apply_relation_delta(
            actor_id,
            target_id,
            &RelationDelta {
                trust: -6,
                friendship: -4,
                resentment: 10,
                attraction: 0,
                moral_debt: -6,
                reputation: -12,
            },
        )?;
        self.add_memory(
            target_id,
            MemoryKind::Offense,
            format!("{actor_name} me atacou fisicamente."),
            vec!["violencia".to_string(), "ofensa".to_string()],
            35,
            vec![actor_id],
        )?;
        self.add_memory(
            actor_id,
            MemoryKind::Offense,
            format!("Eu ataquei {target_name}."),
            vec!["violencia".to_string(), "culpa".to_string()],
            24,
            vec![target_id],
        )?;

        if target_died {
            self.mark_agent_dead(target_id, &format!("morto por {actor_name}"))?;
            self.open_crime_case_if_observed(
                CrimeType::Homicide,
                Some(target_id),
                Some(actor_id),
                100,
                vec!["corpo e ferimentos fatais".to_string()],
                true,
            )?;
        } else {
            self.ensure_combat(actor_id, target_id)?;
            let victim_conscious = self.life_status(target_id)? == AgentLifeStatus::Vivo;
            self.open_crime_case_if_observed(
                CrimeType::Assault,
                Some(target_id),
                Some(actor_id),
                if target_incapacitated { 70 } else { 45 },
                vec!["ferimentos visiveis".to_string()],
                victim_conscious,
            )?;
        }

        // Trauma traits for victim
        let event_kind = if target_died { EventKind::Death } else { EventKind::Violence };
        self.apply_trauma_traits_for_event(target_id, "victim", event_kind)?;

        // Witness contagion
        let actor_building = self
            .find_agent_entity(actor_id)
            .ok()
            .and_then(|e| self.world.entity(e).get::<PositionComponent>().map(|p| p.0))
            .and_then(|pos| self.tile_at(pos).and_then(|t| t.building_id));
        self.propagate_witness_effects(actor_building, actor_id, target_id, event_kind)?;

        Ok(())
    }

    fn apply_robbery_intent(&mut self, actor_id: u64, target_id: Option<u64>) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        if !self.agents_adjacent(actor_id, target_id)? {
            return Ok(());
        }
        let stolen = self.transfer_stolen_material(actor_id, target_id, true)?;
        self.apply_attack(actor_id, target_id, false)?;
        if !stolen.is_empty() {
            let victim_conscious = self.life_status(target_id)? == AgentLifeStatus::Vivo;
            self.open_crime_case_if_observed(
                CrimeType::Robbery,
                Some(target_id),
                Some(actor_id),
                75,
                stolen.clone(),
                victim_conscious,
            )?;
            let actor_name = self.agent_name(actor_id)?;
            let target_name = self.agent_name(target_id)?;
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: actor_id,
                target: Some(target_id),
                kind: EventKind::Theft,
                summary: format!("{actor_name} rouba {} de {target_name}.", stolen.join(", ")),
                impact_tags: vec!["roubo".to_string(), "crime".to_string()],
            });
        }

        // Trauma traits for theft victim
        self.apply_trauma_traits_for_event(target_id, "victim", EventKind::Theft)?;

        // Witness contagion for robbery
        let actor_building = self
            .find_agent_entity(actor_id)
            .ok()
            .and_then(|e| self.world.entity(e).get::<PositionComponent>().map(|p| p.0))
            .and_then(|pos| self.tile_at(pos).and_then(|t| t.building_id));
        self.propagate_witness_effects(actor_building, actor_id, target_id, EventKind::Theft)?;

        Ok(())
    }

    fn apply_theft_intent(&mut self, actor_id: u64, target_id: Option<u64>) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        let actor_pos = self.agent_position(actor_id)?;
        let Some(distance) = self.agent_distance_from(actor_pos, target_id) else {
            return Ok(());
        };
        if distance > 2 {
            return Ok(());
        }
        let stolen = self.transfer_stolen_material(actor_id, target_id, false)?;
        if stolen.is_empty() {
            return Ok(());
        }
        let witnesses = self.witnesses_near(actor_id, actor_pos, 4);
        let observed_by_victim =
            distance == 1 && self.life_status(target_id)? == AgentLifeStatus::Vivo;
        self.open_crime_case_if_observed(
            CrimeType::Theft,
            Some(target_id),
            Some(actor_id),
            35,
            if witnesses.is_empty() && !observed_by_victim {
                Vec::new()
            } else {
                stolen.clone()
            },
            observed_by_victim,
        )?;
        self.apply_relation_delta(
            target_id,
            actor_id,
            &RelationDelta {
                trust: -16,
                friendship: -8,
                resentment: 18,
                attraction: 0,
                moral_debt: 0,
                reputation: -8,
            },
        )?;
        let actor_name = self.agent_name(actor_id)?;
        let target_name = self.agent_name(target_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: Some(target_id),
            kind: EventKind::Theft,
            summary: format!("{actor_name} furta {} de {target_name}.", stolen.join(", ")),
            impact_tags: vec!["furto".to_string(), "crime".to_string()],
        });

        // Trauma traits for theft victim
        self.apply_trauma_traits_for_event(target_id, "victim", EventKind::Theft)?;

        Ok(())
    }

    fn apply_flee_intent(&mut self, actor_id: u64) -> Result<()> {
        let actor_name = self.agent_name(actor_id)?;
        let active_combat_ids = self
            .combats
            .iter()
            .filter(|combat| {
                combat.status == CombatStatus::Active && combat.participants.contains(&actor_id)
            })
            .map(|combat| combat.id)
            .collect::<Vec<_>>();
        for combat_id in active_combat_ids {
            if let Some(combat) = self
                .combats
                .iter_mut()
                .find(|combat| combat.id == combat_id)
            {
                combat.status = CombatStatus::Ended;
                combat.outcome = CombatOutcome::Fled;
                combat.end_reason = Some(format!("{actor_name} fugiu do combate"));
            }
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: EventKind::Travel,
            summary: format!("{actor_name} tenta fugir do perigo."),
            impact_tags: vec!["fuga".to_string(), "combate".to_string()],
        });
        Ok(())
    }

    fn apply_accuse_intent(&mut self, actor_id: u64, target_id: Option<u64>) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        let actor_name = self.agent_name(actor_id)?;
        let target_name = self.agent_name(target_id)?;
        let mut updated = false;
        for case in self
            .crime_cases
            .iter_mut()
            .filter(|case| case.suspect_id == Some(target_id))
        {
            case.confidence = (case.confidence + 10).min(100);
            if case.status == CrimeCaseStatus::Open {
                case.status = CrimeCaseStatus::Investigating;
            }
            updated = true;
        }
        if !updated {
            self.open_crime_case_if_observed(
                CrimeType::Theft,
                Some(actor_id),
                Some(target_id),
                25,
                vec![format!("acusacao de {actor_name}")],
                true,
            )?;
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: Some(target_id),
            kind: EventKind::CrimeReported,
            summary: format!("{actor_name} acusa {target_name} diante da vila."),
            impact_tags: vec!["acusacao".to_string(), "justica".to_string()],
        });
        Ok(())
    }

    fn apply_investigate_intent(&mut self, actor_id: u64) -> Result<()> {
        if !self.has_justice_authority(actor_id)? {
            return Ok(());
        }
        let actor_name = self.agent_name(actor_id)?;
        let Some((case_id, suspect_id)) = self
            .crime_cases
            .iter_mut()
            .find(|case| {
                matches!(
                    case.status,
                    CrimeCaseStatus::Open | CrimeCaseStatus::Investigating
                )
            })
            .map(|case| {
                case.status = CrimeCaseStatus::Investigating;
                case.confidence = (case.confidence + 25).min(100);
                if case.confidence >= 70 {
                    case.status = CrimeCaseStatus::Proven;
                }
                (case.id, case.suspect_id)
            })
        else {
            return Ok(());
        };
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: suspect_id,
            kind: EventKind::Investigation,
            summary: format!("{actor_name} investiga o caso criminal {case_id}."),
            impact_tags: vec!["investigacao".to_string(), "justica".to_string()],
        });
        Ok(())
    }

    fn apply_arrest_intent(&mut self, actor_id: u64, target_id: Option<u64>) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        if !self.has_justice_authority(actor_id)? || !self.agents_adjacent(actor_id, target_id)? {
            return Ok(());
        }
        let Some(case) = self.crime_cases.iter_mut().find(|case| {
            case.suspect_id == Some(target_id)
                && case.confidence >= 60
                && matches!(
                    case.status,
                    CrimeCaseStatus::Investigating | CrimeCaseStatus::Proven
                )
        }) else {
            return Ok(());
        };
        case.status = CrimeCaseStatus::Arrested;
        let case_id = case.id;
        if let Some(guard_post) = self
            .spatial
            .buildings
            .iter()
            .find(|building| building.kind == LocationKind::GuardPost)
            .map(|building| building.entrance)
        {
            self.force_agent_position(target_id, guard_post)?;
        }
        let actor_name = self.agent_name(actor_id)?;
        let target_name = self.agent_name(target_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: Some(target_id),
            kind: EventKind::Arrest,
            summary: format!("{actor_name} prende {target_name} pelo caso {case_id}."),
            impact_tags: vec!["prisao".to_string(), "justica".to_string()],
        });
        Ok(())
    }

    fn apply_punish_intent(&mut self, actor_id: u64, target_id: Option<u64>) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        if !self.has_justice_authority(actor_id)? {
            return Ok(());
        }
        let justice_severity = self.local_norms.justice_severity;
        let Some(case) = self.crime_cases.iter_mut().find(|case| {
            case.suspect_id == Some(target_id)
                && matches!(
                    case.status,
                    CrimeCaseStatus::Arrested | CrimeCaseStatus::Proven
                )
        }) else {
            return Ok(());
        };
        case.status = CrimeCaseStatus::Punished;
        let severity = case.severity;
        let sentence_for_norm = sentence_for_case_severity(justice_severity, severity);
        case.sentence = sentence_for_norm;
        let sentence = case.sentence;
        let case_id = case.id;
        if matches!(sentence, SentenceKind::Fine | SentenceKind::Restitution)
            && let Some(household_id) = self.household_id_for_agent(target_id)
            && let Some(household) = self.household_by_id_mut(household_id)
        {
            let paid = household.treasury.min(3);
            household.treasury -= paid;
            self.village_economy.public_treasury += paid;
        }
        let actor_name = self.agent_name(actor_id)?;
        let target_name = self.agent_name(target_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: Some(target_id),
            kind: EventKind::Punishment,
            summary: format!(
                "{actor_name} pune {target_name} no caso {case_id} com {:?}.",
                sentence
            ),
            impact_tags: vec!["punicao".to_string(), "justica".to_string()],
        });

        // Trauma traits for punished agent
        self.apply_trauma_traits_for_event(target_id, "victim", EventKind::Punishment)?;

        Ok(())
    }

    fn apply_political_support_intent(&mut self, actor_id: u64, support: bool) -> Result<()> {
        let Some(issue_id) = self.preferred_political_issue_for_actor(actor_id) else {
            return Ok(());
        };
        self.record_political_position(actor_id, issue_id, support)?;
        let actor_name = self.agent_name(actor_id)?;
        let issue_summary = self
            .political_issues
            .iter()
            .find(|issue| issue.id == issue_id)
            .map(|issue| issue.summary.clone())
            .unwrap_or_else(|| format!("pauta {issue_id}"));
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: EventKind::PoliticalSupport,
            summary: format!(
                "{actor_name} {} a pauta: {issue_summary}.",
                if support { "apoia" } else { "se opoe a" }
            ),
            impact_tags: vec!["politica".to_string(), "apoio".to_string()],
        });
        Ok(())
    }

    fn apply_political_pressure_intent(
        &mut self,
        actor_id: u64,
        target_id: Option<u64>,
    ) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        if !self.agents_adjacent(actor_id, target_id)? {
            return Ok(());
        }
        let Some(issue_id) = self.preferred_political_issue_for_actor(actor_id) else {
            return Ok(());
        };
        self.record_political_position(actor_id, issue_id, true)?;
        let influence = (self.political_influence(actor_id) / 3).max(1);
        if let Some(issue) = self
            .political_issues
            .iter_mut()
            .find(|issue| issue.id == issue_id)
        {
            issue.support_score += influence;
        }
        self.apply_relation_delta(
            target_id,
            actor_id,
            &RelationDelta {
                trust: -3,
                friendship: -2,
                resentment: 5,
                attraction: 0,
                moral_debt: -2,
                reputation: -1,
            },
        )?;
        let actor_name = self.agent_name(actor_id)?;
        let target_name = self.agent_name(target_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: Some(target_id),
            kind: EventKind::InstitutionalDispute,
            summary: format!("{actor_name} pressiona {target_name} em disputa institucional."),
            impact_tags: vec!["politica".to_string(), "pressao".to_string()],
        });
        Ok(())
    }

    fn apply_political_request_support_intent(
        &mut self,
        actor_id: u64,
        target_id: Option<u64>,
    ) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        if !self.agents_adjacent(actor_id, target_id)? {
            return Ok(());
        }
        let Some(issue_id) = self.preferred_political_issue_for_actor(actor_id) else {
            return Ok(());
        };
        self.record_political_position(actor_id, issue_id, true)?;
        let relation = self.relation_between(target_id, actor_id);
        let persuaded = relation.trust + relation.friendship - relation.resentment >= -10;
        self.record_political_position(target_id, issue_id, persuaded)?;
        self.apply_relation_delta(
            actor_id,
            target_id,
            &RelationDelta {
                trust: if persuaded { 2 } else { -2 },
                friendship: if persuaded { 2 } else { -1 },
                resentment: if persuaded { -1 } else { 3 },
                attraction: 0,
                moral_debt: if persuaded { 1 } else { 0 },
                reputation: 0,
            },
        )?;
        let actor_name = self.agent_name(actor_id)?;
        let target_name = self.agent_name(target_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: Some(target_id),
            kind: EventKind::PoliticalSupport,
            summary: if persuaded {
                format!("{actor_name} convence {target_name} a apoiar uma pauta local.")
            } else {
                format!("{actor_name} pede apoio a {target_name}, mas encontra resistencia.")
            },
            impact_tags: vec!["politica".to_string(), "apoio".to_string()],
        });
        Ok(())
    }

    fn apply_political_mediate_intent(
        &mut self,
        actor_id: u64,
        target_id: Option<u64>,
    ) -> Result<()> {
        if let Some(target_id) = target_id
            && !self.agents_adjacent(actor_id, target_id)?
        {
            return Ok(());
        }
        if self.political_influence(actor_id) < 18 {
            return Ok(());
        }
        let Some(issue) = self
            .political_issues
            .iter_mut()
            .filter(|issue| issue.status == PoliticalIssueStatus::Open)
            .max_by_key(|issue| (issue.support_score - issue.opposition_score).abs())
        else {
            return Ok(());
        };
        issue.support_score = (issue.support_score - 4).max(0);
        issue.opposition_score = (issue.opposition_score - 4).max(0);
        let actor_name = self.agent_name(actor_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: target_id,
            kind: EventKind::InstitutionalDispute,
            summary: format!("{actor_name} medeia uma disputa politica e reduz a polarizacao."),
            impact_tags: vec!["politica".to_string(), "mediacao".to_string()],
        });
        Ok(())
    }

    fn can_receive_violence(&mut self, agent_id: u64) -> Result<bool> {
        let status = self.life_status(agent_id)?;
        Ok(status != AgentLifeStatus::Morto)
    }

    fn life_status(&mut self, agent_id: u64) -> Result<AgentLifeStatus> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<LifeStatusComponent>()
            .ok_or_else(|| anyhow!("missing life status component"))?
            .0)
    }

    fn agent_position(&mut self, agent_id: u64) -> Result<TileCoord> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<PositionComponent>()
            .ok_or_else(|| anyhow!("missing position component"))?
            .0)
    }

    fn interrupt_agent_conversations(
        &mut self,
        agent_id: u64,
        outcome: ConversationOutcome,
    ) -> Result<()> {
        let conversation_ids = self
            .conversations
            .iter()
            .filter(|conversation| {
                conversation.status == ConversationStatus::Active
                    && conversation.participants.contains(&agent_id)
            })
            .map(|conversation| conversation.id)
            .collect::<Vec<_>>();
        for conversation_id in conversation_ids {
            self.end_conversation(
                conversation_id,
                ConversationStatus::Interrupted,
                outcome.clone(),
                "interrompida por violencia fisica".to_string(),
            )?;
        }
        Ok(())
    }

    fn mark_agent_dead(&mut self, agent_id: u64, reason: &str) -> Result<()> {
        if self.life_status(agent_id)? == AgentLifeStatus::Morto {
            return Ok(());
        }
        let entity = self.find_agent_entity(agent_id)?;
        {
            let mut entity_mut = self.world.entity_mut(entity);
            entity_mut
                .get_mut::<LifeStatusComponent>()
                .ok_or_else(|| anyhow!("missing life status component"))?
                .0 = AgentLifeStatus::Morto;
            entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?
                .0
                .health = 0;
            entity_mut
                .get_mut::<IntentComponent>()
                .ok_or_else(|| anyhow!("missing intent component"))?
                .0 = None;
            entity_mut
                .get_mut::<PathComponent>()
                .ok_or_else(|| anyhow!("missing path component"))?
                .0
                .clear();
            entity_mut
                .get_mut::<DestinationComponent>()
                .ok_or_else(|| anyhow!("missing destination component"))?
                .0 = None;
            entity_mut
                .get_mut::<DestinationLabelComponent>()
                .ok_or_else(|| anyhow!("missing destination label component"))?
                .0 = None;
        }
        self.clear_active_economic_task(agent_id)?;
        self.interrupt_agent_conversations(agent_id, ConversationOutcome::PhysicalConflict)?;
        for combat in self.combats.iter_mut().filter(|combat| {
            combat.status == CombatStatus::Active && combat.participants.contains(&agent_id)
        }) {
            combat.status = CombatStatus::Ended;
            combat.outcome = CombatOutcome::Death;
            combat.end_reason = Some(reason.to_string());
        }
        let agent_name = self.agent_name(agent_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::Death,
            summary: format!("{agent_name} morre: {reason}."),
            impact_tags: vec!["morte".to_string(), "violencia".to_string()],
        });
        Ok(())
    }

    fn ensure_combat(&mut self, actor_id: u64, target_id: u64) -> Result<()> {
        if self.combats.iter().any(|combat| {
            combat.status == CombatStatus::Active
                && combat.participants.contains(&actor_id)
                && combat.participants.contains(&target_id)
        }) {
            return Ok(());
        }
        let id = self.next_combat_id;
        self.next_combat_id += 1;
        self.combats.push(CombatState {
            id,
            participants: [actor_id, target_id],
            aggressor_id: actor_id,
            started_at_tick: self.total_ticks,
            round: 0,
            status: CombatStatus::Active,
            outcome: CombatOutcome::Ongoing,
            end_reason: None,
        });
        Ok(())
    }

    fn open_crime_case_if_observed(
        &mut self,
        crime_type: CrimeType,
        victim_id: Option<u64>,
        suspect_id: Option<u64>,
        severity: u8,
        evidence: Vec<String>,
        victim_conscious: bool,
    ) -> Result<Option<CrimeCaseId>> {
        let origin = suspect_id
            .and_then(|id| self.agent_position(id).ok())
            .or_else(|| victim_id.and_then(|id| self.agent_position(id).ok()))
            .unwrap_or(TileCoord { x: 0, y: 0 });
        let witnesses = suspect_id
            .map(|suspect| self.witnesses_near(suspect, origin, 5))
            .unwrap_or_default()
            .into_iter()
            .filter(|id| Some(*id) != victim_id)
            .collect::<Vec<_>>();
        if witnesses.is_empty() && evidence.is_empty() && !victim_conscious {
            return Ok(None);
        }
        if let Some(existing) = self.crime_cases.iter_mut().find(|case| {
            case.crime_type == crime_type
                && case.victim_id == victim_id
                && case.suspect_id == suspect_id
                && !matches!(
                    case.status,
                    CrimeCaseStatus::Punished | CrimeCaseStatus::Closed
                )
        }) {
            existing.confidence = (existing.confidence + 15).min(100);
            existing.severity = existing.severity.max(severity);
            for witness in witnesses {
                if !existing.witnesses.contains(&witness) {
                    existing.witnesses.push(witness);
                }
            }
            for item in evidence {
                if !existing.evidence.contains(&item) {
                    existing.evidence.push(item);
                }
            }
            return Ok(Some(existing.id));
        }

        let id = self.next_crime_case_id;
        self.next_crime_case_id += 1;
        let confidence = (if victim_conscious { 35 } else { 0 }
            + witnesses.len() as u8 * 20
            + evidence.len() as u8 * 15)
            .min(100);
        let summary = format!(
            "{:?} envolvendo vitima={:?} suspeito={:?}",
            crime_type, victim_id, suspect_id
        );
        self.crime_cases.push(CrimeCase {
            id,
            crime_type,
            victim_id,
            suspect_id,
            witnesses: witnesses.clone(),
            evidence,
            severity,
            confidence,
            status: CrimeCaseStatus::Open,
            sentence: SentenceKind::None,
            opened_day: self.day,
            opened_tick: self.tick_of_day,
            summary: summary.clone(),
        });
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: suspect_id.unwrap_or(0),
            target: victim_id,
            kind: EventKind::CrimeReported,
            summary: format!("Caso criminal {id} aberto: {summary}."),
            impact_tags: vec!["crime".to_string(), "justica".to_string()],
        });
        Ok(Some(id))
    }

    fn transfer_stolen_material(
        &mut self,
        actor_id: u64,
        target_id: u64,
        violent: bool,
    ) -> Result<Vec<String>> {
        let mut stolen = Vec::new();
        let target_entity = self.find_agent_entity(target_id)?;
        let actor_entity = self.find_agent_entity(actor_id)?;
        let actor_capacity = self
            .world
            .entity(actor_entity)
            .get::<EconomicActivityComponent>()
            .ok_or_else(|| anyhow!("missing economy component"))?
            .carrying_capacity;
        let actor_load: i32 = self
            .world
            .entity(actor_entity)
            .get::<EconomicActivityComponent>()
            .ok_or_else(|| anyhow!("missing economy component"))?
            .carrying
            .iter()
            .map(|stack| stack.amount)
            .sum();
        let capacity_left = (actor_capacity - actor_load).max(0);
        if capacity_left > 0 {
            let taken_stack = {
                let mut target_mut = self.world.entity_mut(target_entity);
                let mut target_economy = target_mut
                    .get_mut::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?;
                target_economy
                    .carrying
                    .iter_mut()
                    .find(|stack| stack.amount > 0)
                    .map(|stack| {
                        stack.amount -= 1;
                        stack.resource_id.clone()
                    })
            };
            if let Some(resource_id) = taken_stack {
                let mut actor_mut = self.world.entity_mut(actor_entity);
                let mut actor_economy = actor_mut
                    .get_mut::<EconomicActivityComponent>()
                    .ok_or_else(|| anyhow!("missing economy component"))?;
                Self::push_resource(&mut actor_economy.carrying, &resource_id, 1);
                stolen.push(format!("1 {}", self.resource_display_name(&resource_id)));
                return Ok(stolen);
            }
        }

        if let (Some(victim_household_id), Some(actor_household_id)) = (
            self.household_id_for_agent(target_id),
            self.household_id_for_agent(actor_id),
        ) {
            let amount = if violent { 3 } else { 1 };
            let taken =
                if let Some(victim_household) = self.household_by_id_mut(victim_household_id) {
                    let taken = victim_household.treasury.min(amount);
                    victim_household.treasury -= taken;
                    taken
                } else {
                    0
                };
            if taken > 0 {
                if let Some(actor_household) = self.household_by_id_mut(actor_household_id) {
                    actor_household.treasury += taken;
                }
                stolen.push(format!("{taken} moeda(s)"));
                return Ok(stolen);
            }
            if capacity_left > 0 {
                let taken_food =
                    if let Some(victim_household) = self.household_by_id_mut(victim_household_id) {
                        victim_household
                            .pantry
                            .iter_mut()
                            .find(|stack| stack.amount > 0)
                            .map(|stack| {
                                stack.amount -= 1;
                                stack.resource_id.clone()
                            })
                    } else {
                        None
                    };
                if let Some(resource_id) = taken_food {
                    let mut actor_mut = self.world.entity_mut(actor_entity);
                    let mut actor_economy = actor_mut
                        .get_mut::<EconomicActivityComponent>()
                        .ok_or_else(|| anyhow!("missing economy component"))?;
                    Self::push_resource(&mut actor_economy.carrying, &resource_id, 1);
                    stolen.push(format!("1 {}", self.resource_display_name(&resource_id)));
                }
            }
        }
        Ok(stolen)
    }

    fn witnesses_near(
        &mut self,
        excluded_agent_id: u64,
        origin: TileCoord,
        radius: i32,
    ) -> Vec<u64> {
        let mut query = self
            .world
            .query::<(&AgentCore, &PositionComponent, &LifeStatusComponent)>();
        query
            .iter(&self.world)
            .filter_map(|(core, position, life)| {
                (core.id != excluded_agent_id
                    && life.0 == AgentLifeStatus::Vivo
                    && position.0.manhattan(origin) <= radius)
                    .then_some(core.id)
            })
            .collect()
    }

    fn agent_distance_from_immutable(&mut self, origin: TileCoord, other_id: u64) -> Option<i32> {
        let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
        query.iter(&self.world).find_map(|(core, position)| {
            (core.id == other_id).then_some(origin.manhattan(position.0))
        })
    }

    fn injury_summary_for_agent(&mut self, agent_id: u64) -> String {
        let mut query = self.world.query::<(&AgentCore, &InjuryComponent)>();
        query
            .iter(&self.world)
            .find_map(|(core, injury)| {
                (core.id == agent_id).then(|| {
                    format!(
                        "leves={} graves={} dor={} sangramento={}",
                        injury.0.light_wounds,
                        injury.0.severe_wounds,
                        injury.0.pain,
                        injury.0.bleeding
                    )
                })
            })
            .unwrap_or_else(|| "sem dados de ferimento".to_string())
    }

    fn has_justice_authority(&mut self, agent_id: u64) -> Result<bool> {
        let entity = self.find_agent_entity(agent_id)?;
        let role_id = self
            .world
            .entity(entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .role_id
            .clone();
        Ok(role_id == Role::Guard.id() || role_id == Role::Headman.id())
    }

    fn force_agent_position(&mut self, agent_id: u64, coord: TileCoord) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<PositionComponent>()
            .ok_or_else(|| anyhow!("missing position component"))?
            .0 = coord;
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
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
                    .role_id
                    .clone(),
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
                    .role_id
                    .clone(),
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
            speaker_role: self.role_display_name(&speaker_role),
            speaker_state: speaker_state.clone(),
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
                role: self.role_display_name(&listener_role),
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
            chaos_pressure: {
                let hh_id = self.household_id_for_agent(speaker_id);
                let hh = hh_id.and_then(|hid| self.household_by_id(hid));
                let hh_treasury = hh.map(|h| h.treasury).unwrap_or(0);
                let food_crisis = hh.map(|h| h.food_crisis_level).unwrap_or(0);
                let injury = self
                    .find_agent_entity(speaker_id)
                    .ok()
                    .and_then(|e| {
                        self.world
                            .entity(e)
                            .get::<InjuryComponent>()
                            .map(|i| i.0.clone())
                    })
                    .unwrap_or_default();
                let relations = self
                    .find_agent_entity(speaker_id)
                    .ok()
                    .and_then(|e| {
                        self.world
                            .entity(e)
                            .get::<RelationComponent>()
                            .map(|r| r.0.clone())
                    })
                    .unwrap_or_default();
                Self::compute_chaos_pressure(
                    &speaker_state,
                    &speaker_profile,
                    &relations,
                    &injury,
                    hh_treasury,
                    food_crisis,
                )
            },
            personality_traits: speaker_profile.traits.clone(),
            trauma_traits: speaker_profile.trauma_traits.clone(),
        })
    }

    fn apply_think_maker_output(&mut self, agent_id: u64, output: ThinkMakerOutput) -> Result<()> {
        // 1. Update ThoughtComponent (reflection)
        self.set_thought(agent_id, output.reflection.clone())?;

        // 2. Find agent entity and update active goals (belief_updates) in StateComponent
        // Also update the active intent's dominant_emotion and justification
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        if let Some(mut state) = entity_mut.get_mut::<StateComponent>() {
            for belief in &output.belief_updates {
                if !state.0.active_goals.iter().any(|goal| goal == belief) {
                    state.0.active_goals.push(belief.clone());
                }
            }
            if state.0.active_goals.len() > 4 {
                state.0.active_goals.truncate(4);
            }
        }

        if let Some(mut intent_comp) = entity_mut.get_mut::<IntentComponent>() {
            if let Some(ref mut intent) = intent_comp.0 {
                intent.dominant_emotion = output.dominant_emotion.clone();
                intent.justification = output.reflection.clone();
                intent.belief_updates = output.belief_updates.clone();
            }
        }
        drop(entity_mut);

        // 3. Add reflection memory
        self.add_memory(
            agent_id,
            MemoryKind::Reflection,
            format!("Reflexao: {}", output.reflection),
            output.belief_updates,
            12,
            Vec::new(),
        )?;

        Ok(())
    }

    fn process_general_decisions(&mut self, llm: &dyn LlmAdapter) -> Result<()> {
        // 1. Process completed background thoughts
        let mut completed_results = Vec::new();
        let mut skipped_results = Vec::new();

        let mut active_thoughts = Vec::new();
        for pending in self.pending_thoughts.drain(..) {
            if pending.handle.is_finished() {
                match pending.handle.join() {
                    Ok(ThinkMakerResult::Completed(res)) => {
                        completed_results.push(res);
                    }
                    Ok(ThinkMakerResult::Skipped(res)) => {
                        skipped_results.push(res);
                    }
                    Err(_) => {
                        return Err(anyhow!(
                            "Background Think Maker thread panicked for agent {}",
                            pending.agent_id
                        ));
                    }
                }
            } else {
                active_thoughts.push(pending);
            }
        }
        self.pending_thoughts = active_thoughts;

        for result in completed_results {
            self.apply_think_maker_output(result.agent_id, result.output)?;
        }

        for result in skipped_results {
            if !result.error.is_transient() {
                if let Some(entity) = self.find_agent_entity(result.agent_id).ok() {
                    if let Some(mut budget) = self
                        .world
                        .entity_mut(entity)
                        .get_mut::<DecisionBudgetComponent>()
                    {
                        budget.cooldown_until = self.total_ticks + 60;
                    }
                }
                eprintln!(
                    "Persistent Think Maker failure for agent {}: {}. Put on 60-tick cooldown.",
                    result.agent_id, result.error
                );
            }
        }

        // 2. Synchronous Action Planning
        let requests = self.prepare_decision_requests()?;
        
        use rayon::prelude::*;
        let planner_results = requests
            .into_par_iter()
            .map(|request| {
                let res = llm.plan_actions(&request.input);
                (request, res)
            })
            .collect::<Vec<_>>();

        for (request, plan_res) in planner_results {
            let agent_id = request.agent_id;
            let input = request.input;

            self.pending_thoughts.retain(|pending| pending.agent_id != agent_id);

            let raw_plan = match plan_res {
                Ok(plan) => plan,
                Err(error) => {
                    if !error.is_transient() {
                        if let Some(entity) = self.find_agent_entity(agent_id).ok() {
                            if let Some(mut budget) = self
                                .world
                                .entity_mut(entity)
                                .get_mut::<DecisionBudgetComponent>()
                            {
                                budget.cooldown_until = self.total_ticks + 60;
                            }
                        }
                        eprintln!(
                            "Persistent Action Planner failure for agent {}: {}. Put on 60-tick cooldown.",
                            agent_id, error
                        );
                    }
                    self.handle_transient_decision_failure(
                        agent_id,
                        &request.cognition_trigger,
                        request.social_opportunity_signature,
                        &error,
                    )?;
                    continue;
                }
            };

            let tasks = parse_action_planner_output(&raw_plan);

            let first_task = tasks.first().cloned();
            if let Some(task) = first_task {
                let intent = AgentIntent {
                    kind: task.kind,
                    target_agent: task.target_agent,
                    target_semantic: task.target_semantic.clone(),
                    justification: "Planejamento instintivo".to_string(),
                    dominant_emotion: "contido".to_string(),
                    perceived_risk: 0,
                    belief_updates: Vec::new(),
                    priority: 1,
                    social_move: task.social_move,
                };
                let validated = validate_intent(intent, &request.nearby_ids);

                let entity = self.find_agent_entity(agent_id)?;
                let mut entity_mut = self.world.entity_mut(entity);
                let mut queue = entity_mut
                    .get_mut::<TaskQueueComponent>()
                    .ok_or_else(|| anyhow!("missing task queue component"))?;
                queue.0.clear();
                for t in tasks.iter().skip(1) {
                    queue.0.push_back(t.clone());
                }
                drop(queue);
                drop(entity_mut);

                self.assign_intent(
                    agent_id,
                    validated,
                    "Pensando...".to_string(),
                )?;
            } else {
                let entity = self.find_agent_entity(agent_id)?;
                let mut entity_mut = self.world.entity_mut(entity);
                entity_mut
                    .get_mut::<TaskQueueComponent>()
                    .ok_or_else(|| anyhow!("missing task queue component"))?
                    .0
                    .clear();
            }

            self.record_cognition_trigger(agent_id, &request.cognition_trigger)?;
            self.record_social_opportunity_signature(
                agent_id,
                request.social_opportunity_signature.clone(),
            )?;

            // 3. Spawn Think Maker in background
            let think_input = ThinkMakerInput {
                decision_input: input,
                planned_tasks: tasks,
            };
            let worker_llm = llm.clone_box();
            let handle = std::thread::spawn(move || {
                match worker_llm.generate_thoughts(&think_input) {
                    Ok(output) => ThinkMakerResult::Completed(CompletedThoughts {
                        agent_id,
                        output,
                    }),
                    Err(error) => ThinkMakerResult::Skipped(SkippedThoughts {
                        agent_id,
                        error,
                    }),
                }
            });

            self.pending_thoughts
                .push(PendingThoughts { agent_id, handle });
        }

        Ok(())
    }

    fn prepare_decision_requests(&mut self) -> Result<Vec<PreparedDecisionRequest>> {
        let contexts = self.collect_contexts();
        let mut requests = Vec::new();

        for context in contexts {
            if context.life_status != AgentLifeStatus::Vivo {
                continue;
            }
            if context.active_conversation_id.is_some() {
                continue;
            }
            if self.should_hold_locked_economic_task(&context) {
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

            let trigger_overrides_cooldown = matches!(
                cognition_trigger.as_str(),
                "necessidade_critica"
                    | "bloqueio_repetido"
                    | "evento_social_direto"
                    | "falha_tarefa_economica"
                    | "sem_intencao"
            );

            if let Some(entity) = self.find_agent_entity(context.id).ok()
                && let Some(budget) = self.world.entity(entity).get::<DecisionBudgetComponent>()
                && budget.cooldown_until > self.total_ticks
                && !trigger_overrides_cooldown
            {
                continue;
            }

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
            let legal_context = self.build_legal_context(&context);
            let political_context = self.build_political_context(&context);
            let input = DecisionInput {
                actor_id: context.id,
                actor_name: context.name.clone(),
                role: self.role_display_name(&context.role_id),
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
                legal_context,
                political_context,
                profile_summary: context.profile_summary(),
                llm_budget_remaining: 24u32.saturating_sub(context.llm_calls as u32),
                chaos_pressure: {
                    let hh = context
                        .household_id
                        .and_then(|hid| self.household_by_id(hid));
                    let hh_treasury = hh.map(|h| h.treasury).unwrap_or(0);
                    let food_crisis = hh.map(|h| h.food_crisis_level).unwrap_or(0);
                    let injury = self
                        .find_agent_entity(context.id)
                        .ok()
                        .and_then(|e| {
                            self.world
                                .entity(e)
                                .get::<InjuryComponent>()
                                .map(|i| i.0.clone())
                        })
                        .unwrap_or_default();
                    Self::compute_chaos_pressure(
                        &context.state,
                        &context.profile,
                        &context.relations,
                        &injury,
                        hh_treasury,
                        food_crisis,
                    )
                },
                personality_traits: context.profile.traits.clone(),
                trauma_traits: context.profile.trauma_traits.clone(),
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

    // Decisions are processed asynchronously via process_general_decisions and std::thread::spawn

    fn run_parallel_conversation_turns(
        &self,
        llm: &dyn LlmAdapter,
        turns: Vec<PreparedConversationTurn>,
    ) -> Result<Vec<ConversationBatchItem>> {
        let mut results = Vec::with_capacity(turns.len());
        for turn in turns {
            let conversation_id = turn.conversation_id;
            match llm.generate_conversation_turn(&turn.input) {
                Ok(output) => {
                    results.push(ConversationBatchItem::Completed(CompletedConversationTurn {
                        conversation_id,
                        speaker_id: turn.speaker_id,
                        listener_id: turn.listener_id,
                        output,
                    }));
                }
                Err(error) => {
                    if error.is_transient() {
                        results.push(ConversationBatchItem::Interrupted(InterruptedConversationTurn {
                            conversation_id,
                            speaker_id: turn.speaker_id,
                            listener_id: turn.listener_id,
                            error,
                        }));
                    } else {
                        return Err(anyhow!(
                            "conversation {} failed: {}",
                            conversation_id,
                            error
                        ));
                    }
                }
            }
        }
        results.sort_by_key(ConversationBatchItem::conversation_id);
        Ok(results)
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
        if context.last_intent.is_none() && context.task_queue.is_empty() {
            return Ok(Some("sem_intencao".to_string()));
        }

        if context.blocked_ticks >= BLOCKED_RECONSIDERATION_TICKS {
            return Ok(Some("bloqueio_repetido".to_string()));
        }

        if self.has_critical_need(context) {
            return Ok(Some("necessidade_critica".to_string()));
        }

        if self.has_direct_social_event(context.id, recent_events) {
            return Ok(Some("evento_social_direto".to_string()));
        }

        // Check if an active economic task has failed
        if let Some(task) = self.active_economic_task_for_agent(context.id) {
            if task.phase == EconomicTaskPhase::Failed {
                return Ok(Some("falha_tarefa_economica".to_string()));
            }
        }

        Ok(None)
    }

    fn context_depth_for_trigger(&self, trigger: &str) -> &'static str {
        match trigger {
            "evento_social_direto"
            | "bloqueio_repetido"
            | "necessidade_critica"
            | "falha_tarefa_economica" => "expanded",
            _ => "normal",
        }
    }

    fn context_limits_for_trigger(&self, trigger: &str) -> (usize, usize, usize, usize) {
        match self.context_depth_for_trigger(trigger) {
            "expanded" => (self.relevant_memory_limit, 4, 4, 5),
            _ => (3, 3, 3, 3),
        }
    }

    fn has_critical_need(&self, context: &AgentContext) -> bool {
        context.state.hunger >= 70 || context.state.energy <= 20 || context.state.stress >= 72
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
                    .map(|stack| {
                        format!(
                            "{} x{}",
                            self.resource_display_name(&stack.resource_id),
                            stack.amount
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let reserved_food = household
            .map(|household| {
                household
                    .reserved_food
                    .iter()
                    .map(|stack| {
                        format!(
                            "{} x{}",
                            self.resource_display_name(&stack.resource_id),
                            stack.amount
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let pending_salary = household
            .map(|household| {
                household
                    .pending_payments
                    .iter()
                    .map(|claim| claim.amount)
                    .sum()
            })
            .unwrap_or(0);
        let tax_pressure = household
            .map(|household| self.village_economy.daily_household_tax + household.tax_arrears)
            .unwrap_or(0);
        let work_obligations = self.work_obligations_for_context(context);
        let local_prices = self
            .local_prices_for_agent(context.position)
            .into_iter()
            .map(|price| {
                format!(
                    "{}={} moedas",
                    self.resource_display_name(&price.resource_id),
                    price.unit_price
                )
            })
            .collect::<Vec<_>>();
        let base_resource_availability = self
            .catalog
            .resources
            .iter()
            .filter(|resource| {
                resource
                    .tags
                    .iter()
                    .any(|tag| tag == "raw_material" || tag == "capital")
            })
            .map(|resource| {
                let total: i32 = self
                    .establishments
                    .iter()
                    .map(|establishment| {
                        Self::total_resource_amount(&establishment.stock, &resource.id)
                    })
                    .sum();
                format!("{} disponivel localmente: {}", resource.display_name, total)
            })
            .collect::<Vec<_>>();
        let scarcity_signals = self
            .village_economy
            .scarcity_metrics
            .iter()
            .filter(|metric| metric.pressure > 0)
            .take(4)
            .map(|metric| {
                format!(
                    "escassez de {} ({})",
                    self.resource_display_name(&metric.resource_id),
                    metric.pressure
                )
            })
            .collect::<Vec<_>>();
        let public_treasury_status = if self.village_economy.public_treasury < 12 {
            format!(
                "caixa publico baixo ({}) e risco de atraso civico",
                self.village_economy.public_treasury
            )
        } else {
            format!(
                "caixa publico estavel ({})",
                self.village_economy.public_treasury
            )
        };
        let open_tasks = context
            .household_id
            .map(|household_id| self.open_tasks_for_household(household_id))
            .unwrap_or_default();
        let has_food_purchase_in_transit = context
            .household_id
            .map(|household_id| {
                self.economic_tasks.iter().any(|task| {
                    task.actor_household_id == household_id
                        && task.creates_household_reserve
                        && task.phase == EconomicTaskPhase::InTransit
                        && task.phase != EconomicTaskPhase::Completed
                        && task.phase != EconomicTaskPhase::Failed
                })
            })
            .unwrap_or(false);
        let open_food_tasks = context
            .household_id
            .map(|household_id| {
                self.economic_tasks
                    .iter()
                    .filter(|task| {
                        task.actor_household_id == household_id
                            && matches!(
                                task.class,
                                EconomicTaskClass::HouseholdFoodPurchase
                                    | EconomicTaskClass::FoodSupplyTransport
                                    | EconomicTaskClass::FoodProduction
                            )
                            && task.phase != EconomicTaskPhase::Completed
                            && task.phase != EconomicTaskPhase::Failed
                    })
                    .count()
            })
            .unwrap_or(0);
        let grain_availability_total: i32 = self
            .establishments
            .iter()
            .map(|establishment| {
                Self::total_resource_amount(&establishment.stock, ResourceKind::Graos.id())
            })
            .sum();
        let external_grain_offer = self
            .market_quote(ResourceKind::Graos.id())
            .map(|quote| format!("graos externos por {} moedas", quote.buy_price));

        EconomicContextInput {
            household_name: household
                .map(|household| household.name.clone())
                .unwrap_or_else(|| "Sem lar".to_string()),
            household_treasury: household.map(|household| household.treasury).unwrap_or(0),
            pantry,
            reserved_food,
            food_crisis_level: household
                .map(|household| household.food_crisis_level)
                .unwrap_or(0),
            reserved_food_workers: household
                .map(|household| household.reserved_food_workers)
                .unwrap_or(0),
            open_food_tasks,
            has_food_purchase_in_transit,
            can_eat_from_reserve: household
                .map(|household| {
                    self.food_resource_ids_sorted()
                        .into_iter()
                        .any(|resource_id| {
                            Self::total_resource_amount(&household.reserved_food, &resource_id) > 0
                        })
                })
                .unwrap_or(false),
            pending_salary,
            tax_pressure,
            work_obligations,
            local_prices,
            base_resource_availability,
            scarcity_signals,
            grain_availability: format!("graos disponiveis localmente: {grain_availability_total}"),
            external_grain_offer,
            public_treasury_status,
            open_tasks,
        }
    }

    fn build_legal_context(&mut self, context: &AgentContext) -> LegalContextInput {
        let active_combat = self
            .combats
            .iter()
            .find(|combat| {
                combat.status == CombatStatus::Active && combat.participants.contains(&context.id)
            })
            .map(|combat| {
                format!(
                    "combate {} contra {}",
                    combat.id,
                    other_participant(&combat.participants, context.id)
                )
            });
        let nearby_threats = context
            .relations
            .iter()
            .filter_map(|(other_id, relation)| {
                let distance = self.agent_distance_from_immutable(context.position, *other_id)?;
                (distance <= 2 && relation.resentment >= 30).then(|| {
                    format!(
                        "agente {} proximo com ressentimento {}",
                        other_id, relation.resentment
                    )
                })
            })
            .take(4)
            .collect::<Vec<_>>();
        let open_cases = self
            .crime_cases
            .iter()
            .filter(|case| {
                matches!(
                    case.status,
                    CrimeCaseStatus::Open
                        | CrimeCaseStatus::Investigating
                        | CrimeCaseStatus::Proven
                        | CrimeCaseStatus::Arrested
                )
            })
            .take(5)
            .map(|case| {
                format!(
                    "caso {} {:?}: suspeito={:?} vitima={:?} severidade={} confianca={}",
                    case.id,
                    case.crime_type,
                    case.suspect_id,
                    case.victim_id,
                    case.severity,
                    case.confidence
                )
            })
            .collect::<Vec<_>>();
        let cases_against_actor = self
            .crime_cases
            .iter()
            .filter(|case| case.suspect_id == Some(context.id))
            .take(4)
            .map(|case| {
                format!(
                    "caso {} {:?} status {:?}",
                    case.id, case.crime_type, case.status
                )
            })
            .collect::<Vec<_>>();
        let cases_involving_actor = self
            .crime_cases
            .iter()
            .filter(|case| {
                case.victim_id == Some(context.id) || case.witnesses.contains(&context.id)
            })
            .take(4)
            .map(|case| {
                format!(
                    "caso {} {:?} status {:?}",
                    case.id, case.crime_type, case.status
                )
            })
            .collect::<Vec<_>>();
        let witness_count = self.witnesses_near(context.id, context.position, 4).len();
        LegalContextInput {
            life_status: format!("{:?}", context.life_status),
            injury_summary: self.injury_summary_for_agent(context.id),
            active_combat,
            nearby_threats,
            open_cases,
            cases_against_actor,
            cases_involving_actor,
            witness_risk: if witness_count > 0 {
                format!("{witness_count} testemunha(s) possiveis por perto")
            } else {
                "sem testemunhas proximas visiveis".to_string()
            },
        }
    }

    fn build_political_context(&self, context: &AgentContext) -> PoliticalContextInput {
        let local_norms = vec![
            format!(
                "imposto diario por lar: {} moeda(s)",
                self.village_economy.daily_household_tax
            ),
            format!("justica: {}", self.local_norms.justice_severity.as_str()),
            format!(
                "racionamento alimentar: {}",
                self.local_norms.rationing_policy.as_str()
            ),
        ];
        let grievances = self.political_grievances_for_agent(context.id);
        let relevant_factions = self
            .political_factions
            .iter()
            .filter(|faction| {
                faction.member_ids.contains(&context.id)
                    || grievances
                        .iter()
                        .any(|grievance| grievance.contains(&faction.agenda_tag))
            })
            .take(4)
            .map(|faction| {
                format!(
                    "#{} {} influencia={} membros={}",
                    faction.id,
                    faction.name,
                    faction.influence,
                    faction.member_ids.len()
                )
            })
            .collect::<Vec<_>>();
        let open_issues = self
            .political_issues
            .iter()
            .filter(|issue| issue.status == PoliticalIssueStatus::Open)
            .take(5)
            .map(|issue| {
                format!(
                    "#{} {} -> {} | apoio={} oposicao={}",
                    issue.id,
                    issue.domain.as_str(),
                    issue.proposed_value,
                    issue.support_score,
                    issue.opposition_score
                )
            })
            .collect::<Vec<_>>();
        let opposition_risks = self
            .political_issues
            .iter()
            .filter(|issue| {
                issue.status == PoliticalIssueStatus::Open
                    && (issue.supporter_ids.contains(&context.id)
                        || issue.opposer_ids.contains(&context.id))
            })
            .take(3)
            .map(|issue| format!("pauta #{} pode gerar oposicao social", issue.id))
            .collect::<Vec<_>>();
        PoliticalContextInput {
            local_norms,
            relevant_factions,
            open_issues,
            likely_position: self.political_position_for_agent(context.id),
            household_grievances: grievances,
            opposition_risks,
        }
    }

    pub fn politics_overview(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "normas | imposto={} | justica={} | racionamento={}",
            self.village_economy.daily_household_tax,
            self.local_norms.justice_severity.as_str(),
            self.local_norms.rationing_policy.as_str()
        )];
        lines.extend(
            self.political_issues
                .iter()
                .filter(|issue| issue.status == PoliticalIssueStatus::Open)
                .take(5)
                .map(|issue| {
                    format!(
                        "pauta #{} {} -> {} | apoio={} oposicao={} | {}",
                        issue.id,
                        issue.domain.as_str(),
                        issue.proposed_value,
                        issue.support_score,
                        issue.opposition_score,
                        issue.summary
                    )
                }),
        );
        lines.extend(self.political_factions.iter().take(5).map(|faction| {
            let active_str = if faction.is_action_active { "ATIVA" } else { "inativa" };
            format!(
                "faccao #{} {} | influencia={} | membros={} | rage={} | status={} | obj={:?}",
                faction.id,
                faction.name,
                faction.influence,
                faction.member_ids.len(),
                faction.rage,
                active_str,
                faction.objective
            )
        }));
        lines
    }

    fn political_position_for_agent(&self, agent_id: u64) -> String {
        let mut positions = Vec::new();
        for issue in self
            .political_issues
            .iter()
            .filter(|issue| issue.status == PoliticalIssueStatus::Open)
        {
            if issue.supporter_ids.contains(&agent_id) {
                positions.push(format!("apoia #{} {}", issue.id, issue.agenda_tag));
            } else if issue.opposer_ids.contains(&agent_id) {
                positions.push(format!("opoe #{} {}", issue.id, issue.agenda_tag));
            }
        }
        if positions.is_empty() {
            self.political_pressures
                .iter()
                .find(|pressure| pressure.actor_id == agent_id)
                .map(|pressure| format!("inclinado a {}", pressure.agenda_tag))
                .unwrap_or_else(|| "sem alinhamento politico forte".to_string())
        } else {
            positions.truncate(3);
            positions.join(" | ")
        }
    }

    fn political_grievances_for_agent(&self, agent_id: u64) -> Vec<String> {
        self.political_pressures
            .iter()
            .filter(|pressure| pressure.actor_id == agent_id)
            .take(4)
            .map(|pressure| {
                format!(
                    "{}:{} intensidade={} ({})",
                    pressure.domain.as_str(),
                    pressure.agenda_tag,
                    pressure.intensity,
                    pressure.reason
                )
            })
            .collect()
    }

    fn work_obligations_for_context(&self, context: &AgentContext) -> Vec<String> {
        let mut obligations = Vec::new();
        if let Some(role_def) = self.role_def(&context.role_id) {
            for establishment in self.establishments.iter().filter(|establishment| {
                role_def
                    .allowed_establishment_type_ids
                    .contains(&establishment.establishment_type_id)
            }) {
                if context.role_id != Role::Farmer.id()
                    && establishment.building_id != self.work_building_id_for_role(&context.role_id)
                {
                    continue;
                }
                for target in &establishment.stock_targets {
                    let current =
                        Self::total_resource_amount(&establishment.stock, &target.resource_id);
                    if current < target.amount {
                        obligations.push(format!(
                            "{} abaixo do alvo em {}",
                            self.resource_display_name(&target.resource_id),
                            establishment.name
                        ));
                    }
                }
            }
        } else if let Some(building_id) = self.work_building_id_for_role(&context.role_id)
            && let Some(establishment) = self.establishment_by_building(building_id)
        {
            for target in &establishment.stock_targets {
                let current =
                    Self::total_resource_amount(&establishment.stock, &target.resource_id);
                if current < target.amount {
                    obligations.push(format!(
                        "{} abaixo do alvo em {}",
                        self.resource_display_name(&target.resource_id),
                        establishment.name
                    ));
                }
            }
        }
        obligations.truncate(4);
        obligations
    }

    fn work_building_id_for_role(&self, role_id: &str) -> Option<BuildingId> {
        let role_def = self.role_def(role_id)?;
        self.establishments
            .iter()
            .find(|establishment| {
                role_def
                    .allowed_establishment_type_ids
                    .contains(&establishment.establishment_type_id)
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
                class: task.class,
                priority: task.priority,
                summary: task.description.clone(),
                resource_id: task.resource_id.clone(),
                amount: task.amount,
                unit_price: (task.unit_price > 0).then_some(task.unit_price),
            })
            .collect::<Vec<_>>();
        tasks.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.summary.cmp(&b.summary))
        });
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
            IntentKind::Agredir
            | IntentKind::Combater
            | IntentKind::Roubar
            | IntentKind::Furtar
            | IntentKind::Fugir
            | IntentKind::Acusar
            | IntentKind::Investigar
            | IntentKind::Prender
            | IntentKind::Punir
            | IntentKind::Apoiar
            | IntentKind::Opor
            | IntentKind::Pressionar
            | IntentKind::PedirApoio
            | IntentKind::Mediar => 1,
            IntentKind::Trabalhar => ROUTINE_RECONSIDERATION_MAX as u64,
        }
    }

    fn blocked_ticks(&mut self, agent_id: u64) -> Result<u32> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<CognitionComponent>()
            .ok_or_else(|| anyhow!("missing cognition component"))?
            .blocked_ticks)
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

        self.apply_faction_recruitment(speaker_id, listener_id)?;

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
        let mut query = self
            .world
            .query::<(&AgentCore, &PositionComponent, &LifeStatusComponent)>();
        query
            .iter(&self.world)
            .filter(|(_, _, life)| life.0 == AgentLifeStatus::Vivo)
            .map(|(core, position, _)| (position.0, core.id))
            .collect()
    }

    fn agent_distance_from(&mut self, origin: TileCoord, other_id: u64) -> Option<i32> {
        let mut query = self.world.query::<(&AgentCore, &PositionComponent)>();
        query.iter(&self.world).find_map(|(core, position)| {
            (core.id == other_id).then_some(origin.manhattan(position.0))
        })
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
                    role: self.role_display_name(&core.role_id),
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
        let food_order = self.food_resource_ids_sorted();
        let accepted = food_order.iter().map(|id| id.as_str()).collect::<Vec<_>>();
        if let Some(household_id) = self.household_id_for_agent(agent_id) {
            if let Some(household) = self.household_by_id_mut(household_id)
                && consume_matching(&mut household.pantry, &accepted)
            {
                return Ok(true);
            }
            if self.consume_reserved_food_for_household(household_id)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn consume_reserved_food_for_household(&mut self, household_id: BuildingId) -> Result<bool> {
        let food_order = self.food_resource_ids_sorted();
        let accepted = food_order.iter().map(|id| id.as_str()).collect::<Vec<_>>();
        let consumed = if let Some(household) = self.household_by_id_mut(household_id) {
            consume_matching(&mut household.reserved_food, &accepted)
        } else {
            false
        };
        if !consumed {
            return Ok(false);
        }

        let reserved_task_id = self
            .economic_tasks
            .iter()
            .find(|task| {
                task.actor_household_id == household_id
                    && task.creates_household_reserve
                    && task.phase == EconomicTaskPhase::InTransit
                    && task.amount > 0
                    && task.resource_id.as_deref() == Some(ResourceKind::Graos.id())
            })
            .map(|task| task.id);
        if let Some(task_id) = reserved_task_id
            && let Some(task) = self
                .economic_tasks
                .iter_mut()
                .find(|task| task.id == task_id)
        {
            task.amount = (task.amount - 1).max(0);
            if task.amount == 0 {
                task.phase = EconomicTaskPhase::Completed;
                task.assigned_agent_id = None;
            }
        }
        Ok(true)
    }

    fn household_has_ready_food_available(&self, household_id: BuildingId) -> bool {
        self.household_by_id(household_id)
            .map(|household| {
                self.food_resource_ids_sorted()
                    .into_iter()
                    .any(|resource_id| {
                        Self::total_resource_amount(&household.pantry, &resource_id) > 0
                    })
            })
            .unwrap_or(false)
    }

    fn household_has_reserved_food_available(&self, household_id: BuildingId) -> bool {
        self.household_by_id(household_id)
            .map(|household| {
                self.food_resource_ids_sorted()
                    .into_iter()
                    .any(|resource_id| {
                        Self::total_resource_amount(&household.reserved_food, &resource_id) > 0
                    })
            })
            .unwrap_or(false)
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
        _ignore_agent_id: Option<u64>,
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
                if !visited.contains(&neighbor) && self.is_walkable(neighbor) {
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
        Ok(actor.manhattan(target) <= 1)
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

    fn agent_role_pairs(&mut self) -> Vec<(u64, String)> {
        let mut query = self.world.query::<&AgentCore>();
        query
            .iter(&self.world)
            .map(|core| (core.id, core.role_id.clone()))
            .collect()
    }

    fn household_id_for_agent_immutable(&mut self, agent_id: u64) -> Option<BuildingId> {
        let mut query = self.world.query::<&AgentCore>();
        query
            .iter(&self.world)
            .find_map(|core| (core.id == agent_id).then_some(core.home_building_id))
            .flatten()
    }

    fn political_influence(&mut self, agent_id: u64) -> i32 {
        let mut query = self
            .world
            .query::<(&AgentCore, &RelationComponent, &LifeStatusComponent)>();
        let Some((role_id, relations, life_status, household_id)) = query
            .iter(&self.world)
            .find_map(|(core, relations, life_status)| {
                (core.id == agent_id).then(|| {
                    (
                        core.role_id.clone(),
                        relations.0.clone(),
                        life_status.0,
                        core.home_building_id,
                    )
                })
            })
        else {
            return 0;
        };
        if life_status != AgentLifeStatus::Vivo {
            return 0;
        }
        let mut influence = 10;
        if role_id == Role::Headman.id() {
            influence += 15;
        } else if role_id == Role::Guard.id() {
            influence += 8;
        }
        let relation_reputation = if relations.is_empty() {
            0
        } else {
            relations
                .values()
                .map(|relation| relation.reputation + relation.trust / 3 - relation.resentment / 3)
                .sum::<i32>()
                / relations.len() as i32
        };
        influence += relation_reputation.clamp(-8, 12);
        if let Some(household_id) = household_id
            && let Some(household) = self.household_by_id(household_id)
        {
            influence += (household.treasury / 12).clamp(0, 10);
            influence -= (household.tax_arrears / 2).clamp(0, 8);
        }
        if self.crime_cases.iter().any(|case| {
            case.suspect_id == Some(agent_id) && !matches!(case.status, CrimeCaseStatus::Closed)
        }) {
            influence -= 8;
        }
        influence.clamp(0, 45)
    }

    fn preferred_political_issue_for_actor(&mut self, actor_id: u64) -> Option<PoliticalIssueId> {
        if let Some(pressure) = self
            .political_pressures
            .iter()
            .find(|pressure| pressure.actor_id == actor_id)
            && let Some(issue) = self.political_issues.iter().find(|issue| {
                issue.status == PoliticalIssueStatus::Open
                    && issue.domain == pressure.domain
                    && issue.proposed_value == pressure.proposed_value
                    && issue.agenda_tag == pressure.agenda_tag
            })
        {
            return Some(issue.id);
        }
        self.political_issues
            .iter()
            .filter(|issue| issue.status == PoliticalIssueStatus::Open)
            .max_by_key(|issue| issue.support_score - issue.opposition_score)
            .map(|issue| issue.id)
    }

    fn record_political_position(
        &mut self,
        actor_id: u64,
        issue_id: PoliticalIssueId,
        support: bool,
    ) -> Result<()> {
        let influence = self.political_influence(actor_id).max(1);
        let Some(issue) = self
            .political_issues
            .iter_mut()
            .find(|issue| issue.id == issue_id && issue.status == PoliticalIssueStatus::Open)
        else {
            return Ok(());
        };
        if support {
            if !issue.supporter_ids.contains(&actor_id) {
                issue.supporter_ids.push(actor_id);
                issue.support_score += influence;
            }
            issue.opposer_ids.retain(|id| *id != actor_id);
        } else {
            if !issue.opposer_ids.contains(&actor_id) {
                issue.opposer_ids.push(actor_id);
                issue.opposition_score += influence;
            }
            issue.supporter_ids.retain(|id| *id != actor_id);
        }
        Ok(())
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
        self.world
            .entity(entity)
            .get::<AgentCore>()?
            .home_building_id
    }

    fn establishment_by_id(
        &self,
        establishment_id: EstablishmentId,
    ) -> Option<&EstablishmentEconomy> {
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

    fn village_index_of_coord(&self, coord: TileCoord) -> usize {
        let centers = [
            TileCoord { x: 75, y: 22 },
            TileCoord { x: 35, y: 72 },
            TileCoord { x: 115, y: 72 },
        ];
        let mut best_index = 0;
        let mut min_dist = i32::MAX;
        for (i, center) in centers.iter().enumerate() {
            let dist = (coord.x - center.x).abs() + (coord.y - center.y).abs();
            if dist < min_dist {
                min_dist = dist;
                best_index = i;
            }
        }
        best_index
    }

    fn village_index_of_establishment(&self, id: EstablishmentId) -> Option<usize> {
        let establishment = self.establishment_by_id(id)?;
        let building_id = establishment.building_id?;
        let building = self.building_by_id(building_id)?;
        Some(self.village_index_of_coord(building.entrance))
    }

    fn village_index_of_household(&self, id: BuildingId) -> Option<usize> {
        let building = self.building_by_id(id)?;
        Some(self.village_index_of_coord(building.entrance))
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
        prices.sort_by_key(|price| (price.resource_id.clone(), price.unit_price));
        prices.truncate(8);
        prices
    }

    fn total_resource_amount(stacks: &[ResourceStack], resource_id: &str) -> i32 {
        stacks
            .iter()
            .filter(|stack| stack.resource_id == resource_id)
            .map(|stack| stack.amount.max(0))
            .sum()
    }

    fn total_food_units(stacks: &[ResourceStack]) -> i32 {
        stacks
            .iter()
            .filter(|stack| matches!(stack.resource_id.as_str(), "graos" | "pao" | "caldo"))
            .map(|stack| stack.amount.max(0))
            .sum()
    }

    fn take_resource(stacks: &mut Vec<ResourceStack>, resource_id: &str, amount: i32) -> i32 {
        if amount <= 0 {
            return 0;
        }
        let mut remaining = amount;
        let mut taken = 0;
        for stack in stacks
            .iter_mut()
            .filter(|stack| stack.resource_id == resource_id)
        {
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

    fn push_resource(stacks: &mut Vec<ResourceStack>, resource_id: &str, amount: i32) {
        if amount > 0 {
            merge_stack(
                stacks,
                ResourceStack {
                    resource_id: resource_id.to_string(),
                    amount,
                },
            );
        }
    }

    fn base_price(&self, resource_id: &str) -> i32 {
        self.catalog
            .resources
            .iter()
            .find(|resource| resource.id == resource_id)
            .map(|resource| resource.base_price)
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
        let households = self.households.clone();
        let updates = households
            .iter()
            .map(|household| {
                let food_units = Self::total_food_units(&household.pantry)
                    + Self::total_food_units(&household.reserved_food);
                let scarcity_pressure = (household.minimum_food_units - food_units).max(0);
                let hungry_members = self.household_member_count_with_need(household.id, 65);
                let critical_hungry_members =
                    self.household_member_count_with_need(household.id, 85);
                let food_crisis_level = if scarcity_pressure <= 0 {
                    0
                } else if scarcity_pressure >= household.minimum_food_units / 2
                    || critical_hungry_members >= 2
                    || (critical_hungry_members >= 1 && hungry_members >= 2)
                {
                    2
                } else {
                    1
                };
                let reserved_food_workers =
                    self.household_assigned_food_support_workers(household.id) as u8;
                (
                    household.id,
                    household.name.clone(),
                    household.member_ids.first().copied().unwrap_or(0),
                    scarcity_pressure,
                    food_crisis_level,
                    reserved_food_workers,
                )
            })
            .collect::<Vec<_>>();

        let current_total_ticks = self.total_ticks;
        for (
            household_id,
            household_name,
            actor_id,
            scarcity_pressure,
            food_crisis_level,
            reserved_food_workers,
        ) in updates
        {
            let mut previous_level = 0;
            if let Some(household) = self.household_by_id_mut(household_id) {
                previous_level = household.food_crisis_level;
                household.scarcity_pressure = scarcity_pressure;
                household.food_crisis_level = food_crisis_level;
                household.reserved_food_workers = reserved_food_workers;
                if food_crisis_level > 0 {
                    household.last_food_shortage_tick = current_total_ticks;
                }
            }
            if food_crisis_level > previous_level {
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: actor_id,
                    target: None,
                    kind: EventKind::Scarcity,
                    summary: format!(
                        "{household_name} entra em crise alimentar nivel {food_crisis_level}."
                    ),
                    impact_tags: vec![
                        "crise_alimentar".to_string(),
                        format!("household:{household_id}"),
                    ],
                });
            }
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

    fn refresh_political_state(&mut self) -> Result<()> {
        self.political_pressures = self.derive_political_pressures();
        self.ensure_political_issues_and_factions()?;
        self.check_faction_founding()?;
        self.update_faction_rage_and_activity()?;
        self.check_faction_resolution()?;
        Ok(())
    }

    fn derive_political_pressures(&mut self) -> Vec<PoliticalPressure> {
        let mut pressures = Vec::new();
        let households = self.households.clone();
        for household in &households {
            for agent_id in &household.member_ids {
                if household.tax_arrears > 0 {
                    pressures.push(PoliticalPressure {
                        actor_id: *agent_id,
                        household_id: Some(household.id),
                        agenda_tag: "reduzir_imposto".to_string(),
                        domain: PolicyDomain::Tax,
                        proposed_value: "reduzir".to_string(),
                        intensity: (household.tax_arrears
                            + self.village_economy.daily_household_tax)
                            .clamp(1, 20),
                        reason: format!(
                            "{} deve {} moeda(s) de imposto",
                            household.name, household.tax_arrears
                        ),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
                if household.food_crisis_level > 0 {
                    pressures.push(PoliticalPressure {
                        actor_id: *agent_id,
                        household_id: Some(household.id),
                        agenda_tag: "priorizar_lares_na_comida".to_string(),
                        domain: PolicyDomain::Rationing,
                        proposed_value: RationingPolicy::HouseholdFirst.as_str().to_string(),
                        intensity: i32::from(household.food_crisis_level) * 8
                            + household.scarcity_pressure.clamp(0, 12),
                        reason: format!(
                            "{} esta em crise alimentar nivel {}",
                            household.name, household.food_crisis_level
                        ),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
            }
        }

        let crime_cases = self.crime_cases.clone();
        for case in &crime_cases {
            if matches!(
                case.status,
                CrimeCaseStatus::Open | CrimeCaseStatus::Investigating | CrimeCaseStatus::Proven
            ) && case.severity >= 50
            {
                if let Some(victim_id) = case.victim_id {
                    pressures.push(PoliticalPressure {
                        actor_id: victim_id,
                        household_id: self.household_id_for_agent_immutable(victim_id),
                        agenda_tag: "endurecer_justica".to_string(),
                        domain: PolicyDomain::Justice,
                        proposed_value: JusticeSeverity::Severe.as_str().to_string(),
                        intensity: i32::from(case.severity / 8).clamp(1, 20),
                        reason: format!("caso criminal {} sem resposta suficiente", case.id),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
            }
            if case.status == CrimeCaseStatus::Punished
                && let Some(suspect_id) = case.suspect_id
            {
                pressures.push(PoliticalPressure {
                    actor_id: suspect_id,
                    household_id: self.household_id_for_agent_immutable(suspect_id),
                    agenda_tag: "abrandar_justica".to_string(),
                    domain: PolicyDomain::Justice,
                    proposed_value: JusticeSeverity::Lenient.as_str().to_string(),
                    intensity: i32::from(case.severity / 10).clamp(1, 16),
                    reason: format!("punicao no caso {} gera ressentimento legal", case.id),
                    day: self.day,
                    tick: self.tick_of_day,
                });
            }
        }

        if self.village_economy.public_treasury < 20 {
            for (agent_id, role_id) in self.agent_role_pairs() {
                if role_id == Role::Guard.id() || role_id == Role::Headman.id() {
                    pressures.push(PoliticalPressure {
                        actor_id: agent_id,
                        household_id: self.household_id_for_agent_immutable(agent_id),
                        agenda_tag: "aumentar_imposto".to_string(),
                        domain: PolicyDomain::Tax,
                        proposed_value: "aumentar".to_string(),
                        intensity: (20 - self.village_economy.public_treasury).clamp(1, 20),
                        reason: "caixa publico baixo ameaca servico civico".to_string(),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
            }
        }
        pressures
    }

    fn ensure_political_issues_and_factions(&mut self) -> Result<()> {
        let mut grouped: HashMap<(PolicyDomain, String, String), Vec<PoliticalPressure>> =
            HashMap::new();
        for pressure in self.political_pressures.clone() {
            grouped
                .entry((
                    pressure.domain,
                    pressure.proposed_value.clone(),
                    pressure.agenda_tag.clone(),
                ))
                .or_default()
                .push(pressure);
        }

        for ((domain, proposed_value, agenda_tag), pressures) in grouped {
            let mut member_ids = pressures
                .iter()
                .map(|pressure| pressure.actor_id)
                .collect::<Vec<_>>();
            member_ids.sort_unstable();
            member_ids.dedup();
            let influence = member_ids
                .iter()
                .map(|agent_id| self.political_influence(*agent_id))
                .sum::<i32>();
            if member_ids.len() < 2 && influence < 25 {
                continue;
            }

            let issue_id = if let Some(issue) = self.political_issues.iter().find(|issue| {
                issue.status == PoliticalIssueStatus::Open
                    && issue.domain == domain
                    && issue.proposed_value == proposed_value
                    && issue.agenda_tag == agenda_tag
            }) {
                issue.id
            } else {
                let issue_id = self.next_political_issue_id;
                self.next_political_issue_id += 1;
                let summary = political_issue_summary(domain, &proposed_value, &agenda_tag);
                self.political_issues.push(PoliticalIssue {
                    id: issue_id,
                    agenda_tag: agenda_tag.clone(),
                    domain,
                    proposed_value: proposed_value.clone(),
                    summary: summary.clone(),
                    proposed_by: member_ids.first().copied(),
                    support_score: influence / 2,
                    opposition_score: 0,
                    supporter_ids: member_ids.clone(),
                    opposer_ids: Vec::new(),
                    status: PoliticalIssueStatus::Open,
                    opened_day: self.day,
                    resolved_day: None,
                });
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: member_ids.first().copied().unwrap_or(0),
                    target: None,
                    kind: EventKind::PolicyProposal,
                    summary: format!("Nova pauta politica: {summary}."),
                    impact_tags: vec!["politica".to_string(), agenda_tag.clone()],
                });
                issue_id
            };

            if let Some(faction) = self.political_factions.iter_mut().find(|faction| faction.agenda_tag == agenda_tag) {
                if !faction.support_issue_ids.contains(&issue_id) {
                    faction.support_issue_ids.push(issue_id);
                }
            }
        }
        Ok(())
    }

    fn close_daily_economy(&mut self) -> Result<()> {
        let daily_tax = self.village_economy.daily_household_tax;
        let current_day = self.day;
        let tax_results = self
            .households
            .iter()
            .map(|household| {
                let boycotted = household.member_ids.iter().any(|&member_id| {
                    self.political_factions.iter().any(|f| {
                        f.is_action_active && f.agenda_tag == "boicote_imposto" && f.member_ids.contains(&member_id)
                    })
                });
                let owed = daily_tax + household.tax_arrears;
                let paid = if boycotted { 0 } else { household.treasury.min(owed.max(0)) };
                let arrears = owed - paid;
                (
                    household.id,
                    household.name.clone(),
                    household.member_ids.first().copied().unwrap_or(0),
                    owed,
                    paid,
                    arrears,
                    boycotted,
                )
            })
            .collect::<Vec<_>>();
        for (household_id, household_name, actor_id, owed, paid, arrears, boycotted) in tax_results {
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
                summary: if boycotted {
                    format!("{household_name} recusa-se a pagar impostos em protesto ativo!")
                } else if paid >= owed {
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
                impact_tags: if boycotted {
                    vec!["imposto".to_string(), "boicote_imposto".to_string()]
                } else {
                    vec!["imposto".to_string(), "caixa_publico".to_string()]
                },
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
        Ok(())
    }

    fn agent_chaos_pressure(&mut self, agent_id: u64) -> Result<u32> {
        let entity = self.find_agent_entity(agent_id)?;
        let entry = self.world.entity(entity);
        let state = entry.get::<StateComponent>().map(|s| &s.0).ok_or_else(|| anyhow!("missing state"))?;
        let profile = entry.get::<ProfileComponent>().map(|p| &p.0).ok_or_else(|| anyhow!("missing profile"))?;
        let relations = entry.get::<RelationComponent>().map(|r| &r.0);
        let default_relations = HashMap::new();
        let relations_ref = relations.unwrap_or(&default_relations);
        let injury = entry.get::<InjuryComponent>().map(|i| &i.0).ok_or_else(|| anyhow!("missing injury"))?;
        let hh_treasury = self.household_id_for_agent(agent_id)
            .and_then(|h_id| self.household_by_id(h_id))
            .map(|h| h.treasury)
            .unwrap_or(0);
        let food_crisis = self.household_id_for_agent(agent_id)
            .and_then(|h_id| self.household_by_id(h_id))
            .map(|h| h.food_crisis_level)
            .unwrap_or(0);
        Ok(Self::compute_chaos_pressure(
            state,
            profile,
            relations_ref,
            injury,
            hh_treasury,
            food_crisis,
        ))
    }

    fn agent_role(&mut self, agent_id: u64) -> Result<String> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .role_id
            .clone())
    }

    fn village_name_by_index(&self, index: usize) -> &str {
        match index {
            0 => &self.village_name,
            1 => "Vale Verde",
            2 => "Pedra Ruiva",
            _ => "Santa Bruma",
        }
    }

    fn check_faction_founding(&mut self) -> Result<()> {
        let agent_ids = self.agent_ids();
        for agent_id in agent_ids {
            let in_faction = self.political_factions.iter().any(|f| f.member_ids.contains(&agent_id));
            if in_faction {
                continue;
            }

            let coord = self.debug_agent_position(agent_id)?;
            let v_idx = self.village_index_of_coord(coord);
            let v_name = self.village_name_by_index(v_idx).to_string();
            let founder_name = self.agent_name(agent_id)?;

            // 1. Motim de Comida (FoodRiot)
            let hunger = self.agent_state(agent_id)?.hunger;
            if hunger >= 75 {
                let farm_building = self.spatial.buildings.iter()
                    .filter(|b| b.kind == LocationKind::Farm)
                    .min_by_key(|b| b.entrance.manhattan(coord))
                    .cloned();
                if let Some(farm) = farm_building {
                    let faction_id = self.next_political_faction_id;
                    self.next_political_faction_id += 1;
                    let name = format!("Revoltados do Celeiro de {}", v_name);
                    let influence = self.political_influence(agent_id);
                    self.political_factions.push(PoliticalFaction {
                        id: faction_id,
                        name: name.clone(),
                        agenda_tag: "motim_comida".to_string(),
                        domain: PolicyDomain::Rationing,
                        proposed_value: "produtores".to_string(),
                        founder_id: agent_id,
                        member_ids: vec![agent_id],
                        influence,
                        support_issue_ids: Vec::new(),
                        opposition_issue_ids: Vec::new(),
                        objective: Some(FactionObjective::FoodRiot {
                            barn_building_id: farm.id,
                            target_grains: 15,
                            grains_stolen: 0,
                        }),
                        is_action_active: false,
                        rage: 10,
                    });
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: agent_id,
                        target: None,
                        kind: EventKind::FactionShift,
                        summary: format!("{} funda a facção '{}' exigindo grãos do celeiro.", founder_name, name),
                        impact_tags: vec!["politica".to_string(), "faccao".to_string(), "motim_comida".to_string()],
                    });
                    continue;
                }
            }

            // 2. Boicote de Impostos (TaxBoycott)
            if let Some(household_id) = self.household_id_for_agent(agent_id) {
                if let Some(household) = self.household_by_id(household_id) {
                    if household.tax_arrears >= 10 {
                        let faction_id = self.next_political_faction_id;
                        self.next_political_faction_id += 1;
                        let name = format!("Liga Anti-Imposto de {}", v_name);
                        let influence = self.political_influence(agent_id);
                        self.political_factions.push(PoliticalFaction {
                            id: faction_id,
                            name: name.clone(),
                            agenda_tag: "boicote_imposto".to_string(),
                            domain: PolicyDomain::Tax,
                            proposed_value: "reduzir".to_string(),
                            founder_id: agent_id,
                            member_ids: vec![agent_id],
                            influence,
                            support_issue_ids: Vec::new(),
                            opposition_issue_ids: Vec::new(),
                            objective: Some(FactionObjective::TaxBoycott {
                                day_activated: self.day,
                            }),
                            is_action_active: false,
                            rage: 10,
                        });
                        self.push_event(WorldEvent {
                            day: self.day,
                            tick: self.tick_of_day,
                            actor: agent_id,
                            target: None,
                            kind: EventKind::FactionShift,
                            summary: format!("{} funda a facção '{}' boicotando o imposto diário.", founder_name, name),
                            impact_tags: vec!["politica".to_string(), "faccao".to_string(), "boicote_imposto".to_string()],
                        });
                        continue;
                    }
                }
            }

            // 3. Derrubar o Líder (DeposeLeader)
            let chaos = self.agent_chaos_pressure(agent_id)?;
            let profile = self.agent_profile(agent_id)?;
            let is_rebel = profile.traits.contains(&"rebelde".to_string())
                || profile.traits.contains(&"vingativo".to_string())
                || profile.traits.contains(&"oportunista".to_string());
            if chaos >= 70 && is_rebel {
                let mut leader_id_opt = None;
                for (a_id, role_id) in self.agent_role_pairs() {
                    if role_id == Role::Headman.id() {
                        let a_pos = self.debug_agent_position(a_id)?;
                        if self.village_index_of_coord(a_pos) == v_idx {
                            leader_id_opt = Some(a_id);
                            break;
                        }
                    }
                }
                if let Some(leader_agent_id) = leader_id_opt {
                    let faction_id = self.next_political_faction_id;
                    self.next_political_faction_id += 1;
                    let name = format!("Rebeldes Conspiradores de {}", v_name);
                    let influence = self.political_influence(agent_id);
                    self.political_factions.push(PoliticalFaction {
                        id: faction_id,
                        name: name.clone(),
                        agenda_tag: "depor_lider".to_string(),
                        domain: PolicyDomain::Justice,
                        proposed_value: "normal".to_string(),
                        founder_id: agent_id,
                        member_ids: vec![agent_id],
                        influence,
                        support_issue_ids: Vec::new(),
                        opposition_issue_ids: Vec::new(),
                        objective: Some(FactionObjective::DeposeLeader {
                            leader_agent_id,
                        }),
                        is_action_active: false,
                        rage: 15,
                    });
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: agent_id,
                        target: Some(leader_agent_id),
                        kind: EventKind::FactionShift,
                        summary: format!("{} funda a facção '{}' conspirando para depor o Líder.", founder_name, name),
                        impact_tags: vec!["politica".to_string(), "faccao".to_string(), "depor_lider".to_string()],
                    });
                    continue;
                }
            }

            // 4. Justiça Vigilante (VigilanteJustice)
            let mut vigilante_case_opt = None;
            for case in &self.crime_cases {
                if case.victim_id == Some(agent_id)
                    && matches!(case.status, CrimeCaseStatus::Open | CrimeCaseStatus::Investigating)
                    && self.day >= case.opened_day + 1
                {
                    if let Some(suspect_id) = case.suspect_id {
                        vigilante_case_opt = Some((suspect_id, case.id));
                        break;
                    }
                }
            }
            if let Some((suspect_agent_id, crime_case_id)) = vigilante_case_opt {
                let faction_id = self.next_political_faction_id;
                self.next_political_faction_id += 1;
                let name = format!("Vigilantes de {}", v_name);
                let influence = self.political_influence(agent_id);
                self.political_factions.push(PoliticalFaction {
                    id: faction_id,
                    name: name.clone(),
                    agenda_tag: "justica_vigilante".to_string(),
                    domain: PolicyDomain::Justice,
                    proposed_value: "severa".to_string(),
                    founder_id: agent_id,
                    member_ids: vec![agent_id],
                    influence,
                    support_issue_ids: Vec::new(),
                    opposition_issue_ids: Vec::new(),
                    objective: Some(FactionObjective::VigilanteJustice {
                        suspect_agent_id,
                        crime_case_id,
                    }),
                    is_action_active: false,
                    rage: 20,
                });
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: Some(suspect_agent_id),
                    kind: EventKind::FactionShift,
                    summary: format!("{} funda a facção '{}' para caçar e punir o suspeito.", founder_name, name),
                    impact_tags: vec!["politica".to_string(), "faccao".to_string(), "justica_vigilante".to_string()],
                });
                continue;
            }

            // 5. Defensores do Erário (Aumentar Imposto)
            if self.village_economy.public_treasury < 20 {
                if let Ok(role_id) = self.agent_role(agent_id) {
                    if role_id == Role::Guard.id() || role_id == Role::Headman.id() {
                        let faction_id = self.next_political_faction_id;
                        self.next_political_faction_id += 1;
                        let name = format!("Defensores do Erário de {}", v_name);
                        let influence = self.political_influence(agent_id);
                        self.political_factions.push(PoliticalFaction {
                            id: faction_id,
                            name: name.clone(),
                            agenda_tag: "aumentar_imposto".to_string(),
                            domain: PolicyDomain::Tax,
                            proposed_value: "aumentar".to_string(),
                            founder_id: agent_id,
                            member_ids: vec![agent_id],
                            influence,
                            support_issue_ids: Vec::new(),
                            opposition_issue_ids: Vec::new(),
                            objective: None,
                            is_action_active: false,
                            rage: 0,
                        });
                        self.push_event(WorldEvent {
                            day: self.day,
                            tick: self.tick_of_day,
                            actor: agent_id,
                            target: None,
                            kind: EventKind::FactionShift,
                            summary: format!("{} funda a facção '{}' para restaurar o tesouro público.", founder_name, name),
                            impact_tags: vec!["politica".to_string(), "faccao".to_string(), "aumentar_imposto".to_string()],
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn update_faction_rage_and_activity(&mut self) -> Result<()> {
        let mut factions = self.political_factions.clone();
        for faction in &mut factions {
            if faction.is_action_active {
                continue;
            }

            let mut delta_rage = 0;
            for &member_id in &faction.member_ids {
                if let Ok(state) = self.agent_state(member_id) {
                    match faction.objective {
                        Some(FactionObjective::FoodRiot { .. }) => {
                            if state.hunger >= 50 {
                                delta_rage += 2;
                            }
                        }
                        Some(FactionObjective::TaxBoycott { .. }) => {
                            if let Some(household_id) = self.household_id_for_agent(member_id) {
                                if let Some(household) = self.household_by_id(household_id) {
                                    if household.tax_arrears > 0 {
                                        delta_rage += 2;
                                    }
                                }
                            }
                        }
                        Some(FactionObjective::DeposeLeader { .. }) => {
                            let chaos = self.agent_chaos_pressure(member_id)?;
                            if chaos >= 50 {
                                delta_rage += 3;
                            }
                        }
                        Some(FactionObjective::VigilanteJustice { suspect_agent_id, .. }) => {
                            let resentment = self.relation_between(member_id, suspect_agent_id).resentment;
                            if resentment >= 20 {
                                delta_rage += 3;
                            }
                        }
                        None => {}
                    }
                }
            }

            faction.rage += delta_rage;
            faction.influence = faction.member_ids.iter().map(|&id| self.political_influence(id)).sum::<i32>();

            let min_members = if self.agent_ids().len() < 8 { 1 } else { 3 };
            if faction.member_ids.len() >= min_members && faction.rage >= 50 {
                faction.is_action_active = true;
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: faction.founder_id,
                    target: None,
                    kind: EventKind::InstitutionalDispute,
                    summary: format!("A facção '{}' ativa ação física no mundo! Objetivo: {:?}", faction.name, faction.objective),
                    impact_tags: vec!["politica".to_string(), "faccao".to_string(), faction.agenda_tag.clone(), "motim".to_string()],
                });
            }
        }
        self.political_factions = factions;
        Ok(())
    }

    fn check_faction_resolution(&mut self) -> Result<()> {
        let mut factions = self.political_factions.clone();
        let mut factions_to_remove = Vec::new();

        for faction in &mut factions {
            if !faction.is_action_active {
                continue;
            }

            let mut resolved = false;
            let mut success = false;
            let mut reason = String::new();

            let mut active_members = 0;
            for &member_id in &faction.member_ids {
                if self.can_agent_act(member_id)? {
                    active_members += 1;
                }
            }

            if active_members == 0 {
                resolved = true;
                success = false;
                reason = "todos os membros foram nocauteados ou detidos pelos guardas".to_string();
            } else if let Some(obj) = faction.objective {
                match obj {
                    FactionObjective::FoodRiot { barn_building_id, target_grains, grains_stolen } => {
                        if grains_stolen >= target_grains {
                            resolved = true;
                            success = true;
                            reason = format!("saquearam com sucesso {} grãos do Celeiro", grains_stolen);
                        } else {
                            if let Some(est) = self.establishment_by_building(barn_building_id) {
                                let available = Self::total_resource_amount(&est.stock, &ResourceKind::Graos.id().to_string());
                                if available == 0 {
                                    resolved = true;
                                    success = false;
                                    reason = "o estoque de grãos do Celeiro acabou completamente".to_string();
                                }
                            }
                        }
                    }
                    FactionObjective::TaxBoycott { day_activated } => {
                        if self.day > day_activated {
                            resolved = true;
                            success = true;
                            reason = "resistiram com sucesso à cobrança de impostos do dia".to_string();
                        }
                    }
                    FactionObjective::DeposeLeader { leader_agent_id } => {
                        let leader_state = self.agent_state(leader_agent_id)?;
                        if leader_state.health < 30 || leader_state.energy < 15 {
                            resolved = true;
                            success = true;
                            reason = "derrubaram com sucesso o Líder local".to_string();
                            self.village_economy.daily_household_tax = 1;
                            self.local_norms.rationing_policy = RationingPolicy::Balanced;
                            if let Ok(leader_entity) = self.find_agent_entity(leader_agent_id) {
                                let mut leader_entity_mut = self.world.entity_mut(leader_entity);
                                let mut core = leader_entity_mut.get_mut::<AgentCore>().unwrap();
                                core.role_id = "normal".to_string();
                            }
                        }
                    }
                    FactionObjective::VigilanteJustice { suspect_agent_id, crime_case_id } => {
                        let suspect_state = self.agent_state(suspect_agent_id)?;
                        if suspect_state.health < 30 || suspect_state.energy < 15 {
                            resolved = true;
                            success = true;
                            reason = "fizeram justiça com as próprias mãos punindo o suspeito".to_string();
                            if let Some(case) = self.crime_cases.iter_mut().find(|c| c.id == crime_case_id) {
                                case.status = CrimeCaseStatus::Punished;
                            }
                        }
                    }
                }
            }

            if resolved {
                faction.is_action_active = false;
                faction.rage = 0;
                for &member_id in &faction.member_ids {
                    self.clear_intent_navigation(member_id)?;
                }
                let outcome_str = if success { "Sucesso" } else { "Fracasso" };
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: faction.founder_id,
                    target: None,
                    kind: EventKind::InstitutionalDispute,
                    summary: format!("Ação da facção '{}' encerrada ({}): {}.", faction.name, outcome_str, reason),
                    impact_tags: vec!["politica".to_string(), "faccao".to_string(), faction.agenda_tag.clone(), "resolvido".to_string()],
                });
                factions_to_remove.push(faction.id);
            }
        }

        factions.retain(|f| !factions_to_remove.contains(&f.id));
        self.political_factions = factions;
        Ok(())
    }

    fn apply_faction_action_overrides(&mut self) -> Result<()> {
        let agent_ids = self.agent_ids();
        for agent_id in agent_ids {
            let active_faction_opt = self.political_factions.iter()
                .find(|f| f.is_action_active && f.member_ids.contains(&agent_id))
                .cloned();

            if let Some(faction) = active_faction_opt {
                if let Some(obj) = faction.objective {
                    let current_pos = self.debug_agent_position(agent_id)?;
                    let mut target_coord = None;
                    let mut target_agent_id = None;

                    match obj {
                        FactionObjective::FoodRiot { barn_building_id, .. } => {
                            if let Some(building) = self.building_by_id(barn_building_id) {
                                target_coord = Some(building.entrance);
                            }
                        }
                        FactionObjective::TaxBoycott { .. } => {
                            let v_idx = self.village_index_of_coord(current_pos);
                            let guard_post = self.spatial.buildings.iter()
                                .find(|b| b.kind == LocationKind::GuardPost && self.village_index_of_coord(b.entrance) == v_idx)
                                .cloned();
                            if let Some(gp) = guard_post {
                                target_coord = Some(gp.entrance);
                            }
                        }
                        FactionObjective::DeposeLeader { leader_agent_id } => {
                            target_agent_id = Some(leader_agent_id);
                            if let Ok(l_pos) = self.debug_agent_position(leader_agent_id) {
                                target_coord = Some(l_pos);
                            }
                        }
                        FactionObjective::VigilanteJustice { suspect_agent_id, .. } => {
                            target_agent_id = Some(suspect_agent_id);
                            if let Ok(s_pos) = self.debug_agent_position(suspect_agent_id) {
                                target_coord = Some(s_pos);
                            }
                        }
                    }

                    if let Some(dest) = target_coord {
                        let entity = self.find_agent_entity(agent_id)?;
                        let is_at_target = current_pos == dest || current_pos.manhattan(dest) <= 1;

                        if is_at_target {
                            let mut intent_kind = IntentKind::Trabalhar;
                            
                            match obj {
                                FactionObjective::FoodRiot { .. } => {
                                    let mut guard_to_fight = None;
                                    for (other_id, role_id) in self.agent_role_pairs() {
                                        if role_id == Role::Guard.id() && self.can_agent_act(other_id)? {
                                            let other_pos = self.debug_agent_position(other_id)?;
                                            if current_pos.manhattan(other_pos) <= 1 {
                                                guard_to_fight = Some(other_id);
                                                break;
                                            }
                                        }
                                    }
                                    if let Some(guard_id) = guard_to_fight {
                                        intent_kind = IntentKind::Agredir;
                                        target_agent_id = Some(guard_id);
                                    } else {
                                        intent_kind = IntentKind::Trabalhar;
                                        target_agent_id = None;
                                    }
                                }
                                FactionObjective::TaxBoycott { .. } => {
                                    let mut guard_to_fight = None;
                                    for (other_id, role_id) in self.agent_role_pairs() {
                                        if role_id == Role::Guard.id() && self.can_agent_act(other_id)? {
                                            let other_pos = self.debug_agent_position(other_id)?;
                                            if current_pos.manhattan(other_pos) <= 1 {
                                                guard_to_fight = Some(other_id);
                                                break;
                                            }
                                        }
                                    }
                                    if let Some(guard_id) = guard_to_fight {
                                        intent_kind = IntentKind::Agredir;
                                        target_agent_id = Some(guard_id);
                                    } else {
                                        intent_kind = IntentKind::Refletir;
                                        target_agent_id = None;
                                    }
                                }
                                FactionObjective::DeposeLeader { leader_agent_id } => {
                                    intent_kind = IntentKind::Agredir;
                                    target_agent_id = Some(leader_agent_id);
                                }
                                FactionObjective::VigilanteJustice { suspect_agent_id, .. } => {
                                    intent_kind = IntentKind::Agredir;
                                    target_agent_id = Some(suspect_agent_id);
                                }
                            }

                            let mut entity_mut = self.world.entity_mut(entity);
                            entity_mut.get_mut::<IntentComponent>().unwrap().0 = Some(AgentIntent {
                                kind: intent_kind,
                                target_agent: target_agent_id,
                                target_semantic: Some(faction.agenda_tag.clone()),
                                justification: format!("Sobrescrita por ação física da facção '{}'", faction.name),
                                dominant_emotion: "furioso".to_string(),
                                perceived_risk: 0,
                                belief_updates: Vec::new(),
                                priority: 1000,
                                social_move: None,
                            });
                            entity_mut.get_mut::<DestinationComponent>().unwrap().0 = Some(dest);
                            entity_mut.get_mut::<PathComponent>().unwrap().0.clear();
                        } else {
                            let path_opt = self.debug_find_path(current_pos, dest, Some(agent_id));
                            let mut entity_mut = self.world.entity_mut(entity);
                            entity_mut.get_mut::<IntentComponent>().unwrap().0 = Some(AgentIntent {
                                kind: IntentKind::Andar,
                                target_agent: None,
                                target_semantic: Some(faction.agenda_tag.clone()),
                                justification: format!("Caminhando para o motim da facção '{}'", faction.name),
                                dominant_emotion: "determinado".to_string(),
                                perceived_risk: 0,
                                belief_updates: Vec::new(),
                                priority: 1000,
                                social_move: None,
                            });
                            entity_mut.get_mut::<DestinationComponent>().unwrap().0 = Some(dest);
                            if let Some(path) = path_opt {
                                entity_mut.get_mut::<PathComponent>().unwrap().0 = path;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn execute_food_riot_steal(&mut self, actor_id: u64) -> Result<()> {
        let current_pos = self.debug_agent_position(actor_id)?;
        let farm_building = self.spatial.buildings.iter()
            .filter(|b| b.kind == LocationKind::Farm)
            .min_by_key(|b| b.entrance.manhattan(current_pos))
            .cloned();

        if let Some(farm) = farm_building {
            if let Some(establishment) = self.establishment_by_building_mut(farm.id) {
                let grains_stolen = Self::take_resource(
                    &mut establishment.stock,
                    &ResourceKind::Graos.id().to_string(),
                    2,
                );

                if grains_stolen > 0 {
                    let entity = self.find_agent_entity(actor_id)?;
                    let mut entity_mut = self.world.entity_mut(entity);
                    let mut inventory = entity_mut.get_mut::<InventoryComponent>().unwrap();
                    Self::push_resource(&mut inventory.0, &ResourceKind::Graos.id().to_string(), grains_stolen);
                    drop(entity_mut);

                    let mut factions = self.political_factions.clone();
                    for faction in &mut factions {
                        if faction.is_action_active && faction.member_ids.contains(&actor_id) {
                            if let Some(FactionObjective::FoodRiot { grains_stolen: ref mut stolen, .. }) = faction.objective {
                                *stolen += grains_stolen;
                            }
                        }
                    }
                    self.political_factions = factions;

                    let name = self.agent_name(actor_id)?;
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: actor_id,
                        target: None,
                        kind: EventKind::Theft,
                        summary: format!("{} saqueia {} grãos do Celeiro durante o motim!", name, grains_stolen),
                        impact_tags: vec!["politica".to_string(), "motim_comida".to_string(), "roubo".to_string()],
                    });
                }
            }
        }
        Ok(())
    }

    fn apply_faction_recruitment(&mut self, speaker_id: u64, listener_id: u64) -> Result<()> {
        let speaker_factions = self.political_factions.iter()
            .filter(|f| f.member_ids.contains(&speaker_id))
            .cloned()
            .collect::<Vec<_>>();

        for faction in speaker_factions {
            if faction.member_ids.contains(&listener_id) {
                continue;
            }

            let listener_in_any_faction = self.political_factions.iter().any(|f| f.member_ids.contains(&listener_id));
            if listener_in_any_faction {
                continue;
            }

            let relation = self.relation_between(listener_id, speaker_id);
            let has_high_trust = relation.trust >= 10 || relation.friendship >= 10;

            let mut joins = has_high_trust;

            if !joins {
                match faction.objective {
                    Some(FactionObjective::FoodRiot { .. }) => {
                        let hunger = self.agent_state(listener_id)?.hunger;
                        if hunger >= 50 {
                            joins = true;
                        }
                    }
                    Some(FactionObjective::TaxBoycott { .. }) => {
                        if let Some(household_id) = self.household_id_for_agent(listener_id) {
                            if let Some(household) = self.household_by_id(household_id) {
                                if household.tax_arrears > 0 {
                                    joins = true;
                                }
                            }
                        }
                    }
                    Some(FactionObjective::DeposeLeader { .. }) => {
                        let profile = self.agent_profile(listener_id)?;
                        let is_rebel = profile.traits.contains(&"rebelde".to_string())
                            || profile.traits.contains(&"vingativo".to_string())
                            || profile.traits.contains(&"oportunista".to_string());
                        let chaos = self.agent_chaos_pressure(listener_id)?;
                        if is_rebel || chaos >= 50 {
                            joins = true;
                        }
                    }
                    Some(FactionObjective::VigilanteJustice { suspect_agent_id, .. }) => {
                        let resentment = self.relation_between(listener_id, suspect_agent_id).resentment;
                        if resentment >= 15 {
                            joins = true;
                        }
                    }
                    None => {}
                }
            }

            if joins {
                let listener_influence = self.political_influence(listener_id);
                let speaker_name = self.agent_name(speaker_id)?;
                let listener_name = self.agent_name(listener_id)?;
                let faction_name = faction.name.clone();
                let faction_id = faction.id;
                let agenda_tag = faction.agenda_tag.clone();

                if let Some(f) = self.political_factions.iter_mut().find(|f| f.id == faction_id) {
                    f.member_ids.push(listener_id);
                    f.influence += listener_influence;
                }

                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: speaker_id,
                    target: Some(listener_id),
                    kind: EventKind::FactionShift,
                    summary: format!("{} convence {} a se juntar à facção '{}'.", speaker_name, listener_name, faction_name),
                    impact_tags: vec!["politica".to_string(), "faccao".to_string(), agenda_tag],
                });
            }
        }
        Ok(())
    }

    fn resolve_daily_politics(&mut self) -> Result<()> {
        self.refresh_political_state()?;
        let open_issue_ids = self
            .political_issues
            .iter()
            .filter(|issue| issue.status == PoliticalIssueStatus::Open)
            .map(|issue| issue.id)
            .collect::<Vec<_>>();
        for issue_id in open_issue_ids {
            let Some(issue_index) = self
                .political_issues
                .iter()
                .position(|issue| issue.id == issue_id)
            else {
                continue;
            };
            let supporting_pressure_actor_ids = self
                .political_pressures
                .iter()
                .filter(|pressure| {
                    let issue = &self.political_issues[issue_index];
                    pressure.domain == issue.domain
                        && pressure.proposed_value == issue.proposed_value
                        && pressure.agenda_tag == issue.agenda_tag
                })
                .map(|pressure| pressure.actor_id)
                .collect::<Vec<_>>();
            let pressure_support = supporting_pressure_actor_ids
                .into_iter()
                .map(|actor_id| self.political_influence(actor_id) / 2)
                .sum::<i32>();
            let issue = &self.political_issues[issue_index];
            let support = issue.support_score + pressure_support;
            let opposition = issue.opposition_score;
            let net = support - opposition;
            let passed = net >= 25;
            let summary = issue.summary.clone();
            if passed {
                let domain = issue.domain;
                let proposed_value = issue.proposed_value.clone();
                self.apply_political_norm_change(domain, &proposed_value)?;
            }
            let actor = {
                let issue = &mut self.political_issues[issue_index];
                issue.support_score = support;
                issue.opposition_score = opposition;
                issue.status = if passed {
                    PoliticalIssueStatus::Passed
                } else {
                    PoliticalIssueStatus::Rejected
                };
                issue.resolved_day = Some(self.day);
                issue.proposed_by.unwrap_or(0)
            };
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor,
                target: None,
                kind: EventKind::InstitutionalDispute,
                summary: if passed {
                    format!("Pauta aprovada: {summary} (saldo politico {net}).")
                } else {
                    format!("Pauta rejeitada: {summary} (saldo politico {net}).")
                },
                impact_tags: vec!["politica".to_string(), "disputa".to_string()],
            });
        }
        Ok(())
    }

    fn apply_political_norm_change(
        &mut self,
        domain: PolicyDomain,
        proposed_value: &str,
    ) -> Result<()> {
        let before = format!(
            "imposto={} justica={} racionamento={}",
            self.village_economy.daily_household_tax,
            self.local_norms.justice_severity.as_str(),
            self.local_norms.rationing_policy.as_str()
        );
        match domain {
            PolicyDomain::Tax => match proposed_value {
                "reduzir" => {
                    self.village_economy.daily_household_tax =
                        (self.village_economy.daily_household_tax - 1).max(1);
                }
                "aumentar" => {
                    self.village_economy.daily_household_tax =
                        (self.village_economy.daily_household_tax + 1).min(5);
                }
                _ => {}
            },
            PolicyDomain::Justice => {
                self.local_norms.justice_severity = match proposed_value {
                    "branda" => JusticeSeverity::Lenient,
                    "severa" => JusticeSeverity::Severe,
                    _ => JusticeSeverity::Normal,
                };
            }
            PolicyDomain::Rationing => {
                self.local_norms.rationing_policy = match proposed_value {
                    "lares" => RationingPolicy::HouseholdFirst,
                    "produtores" => RationingPolicy::ProducersFirst,
                    "civico" => RationingPolicy::CivicFirst,
                    _ => RationingPolicy::Balanced,
                };
            }
        }
        let after = format!(
            "imposto={} justica={} racionamento={}",
            self.village_economy.daily_household_tax,
            self.local_norms.justice_severity.as_str(),
            self.local_norms.rationing_policy.as_str()
        );
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: 0,
            target: None,
            kind: EventKind::NormChanged,
            summary: format!("Norma local alterada: {before} -> {after}."),
            impact_tags: vec!["politica".to_string(), "norma".to_string()],
        });
        Ok(())
    }

    fn recalculate_posted_prices(&self, establishment: &EstablishmentEconomy) -> Vec<PostedPrice> {
        establishment
            .stock_targets
            .iter()
            .map(|target| {
                let current =
                    Self::total_resource_amount(&establishment.stock, &target.resource_id);
                let shortage = (target.amount - current).max(0);
                let mut unit_price = self.base_price(&target.resource_id) + shortage / 2;
                if establishment.cash < 10 {
                    unit_price += 1;
                }
                if let Some(quote) = self.market_quote(&target.resource_id) {
                    unit_price = unit_price.clamp(quote.sell_price.max(1), quote.buy_price.max(1));
                }
                PostedPrice {
                    resource_id: target.resource_id.clone(),
                    unit_price: unit_price.max(1),
                }
            })
            .collect()
    }

    fn compute_scarcity_metrics(&self) -> Vec<ScarcityMetric> {
        let mut metrics = Vec::new();
        for resource in self
            .catalog
            .resources
            .iter()
            .filter(|resource| !resource.tags.iter().any(|tag| tag == "currency"))
        {
            let available: i32 = self
                .establishments
                .iter()
                .map(|establishment| {
                    Self::total_resource_amount(&establishment.stock, &resource.id)
                })
                .sum::<i32>()
                + self
                    .households
                    .iter()
                    .map(|household| Self::total_resource_amount(&household.pantry, &resource.id))
                    .sum::<i32>();
            let target: i32 = self
                .establishments
                .iter()
                .map(|establishment| self.stock_target_amount(establishment, &resource.id))
                .sum();
            metrics.push(ScarcityMetric {
                resource_id: resource.id.clone(),
                pressure: (target - available).max(0),
            });
        }
        metrics
    }

    fn ensure_economic_tasks(&mut self) {
        self.economic_tasks.retain(|task| {
            task.phase != EconomicTaskPhase::Completed && task.phase != EconomicTaskPhase::Failed
        });
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
        resource_id: Option<&str>,
        destination: &EconomicNode,
    ) -> bool {
        self.economic_tasks.iter().any(|task| {
            task.actor_household_id == household_id
                && task.kind == kind
                && task.resource_id.as_deref() == resource_id
                && task.phase != EconomicTaskPhase::Completed
                && task.phase != EconomicTaskPhase::Failed
                && &task.destination == destination
        })
    }

    fn open_task_count_for_household_class(
        &self,
        household_id: BuildingId,
        class: EconomicTaskClass,
    ) -> usize {
        self.economic_tasks
            .iter()
            .filter(|task| {
                task.actor_household_id == household_id
                    && task.class == class
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
            })
            .count()
    }

    fn matching_open_task_count(
        &self,
        household_id: BuildingId,
        kind: EconomicTaskKind,
        resource_id: Option<&str>,
        destination: &EconomicNode,
        source: Option<&EconomicNode>,
    ) -> usize {
        self.economic_tasks
            .iter()
            .filter(|task| {
                task.actor_household_id == household_id
                    && task.kind == kind
                    && task.resource_id.as_deref() == resource_id
                    && task.destination == *destination
                    && source
                        .map(|expected| task.source == *expected)
                        .unwrap_or(true)
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
            })
            .count()
    }

    fn village_food_pressure(&self) -> i32 {
        let household_pressure: i32 = self
            .households
            .iter()
            .map(|household| household.scarcity_pressure)
            .sum();
        let market_pressure: i32 = self
            .village_economy
            .scarcity_metrics
            .iter()
            .filter(|metric| self.is_food_resource(&metric.resource_id))
            .map(|metric| metric.pressure)
            .sum();
        household_pressure + market_pressure
    }

    fn household_member_count_with_need(
        &mut self,
        household_id: BuildingId,
        hunger_at_least: i32,
    ) -> usize {
        let member_ids = self
            .household_by_id(household_id)
            .map(|household| household.member_ids.clone())
            .unwrap_or_default();
        member_ids
            .into_iter()
            .filter(|agent_id| {
                self.agent_state(*agent_id)
                    .map(|state| state.hunger >= hunger_at_least)
                    .unwrap_or(false)
            })
            .count()
    }

    fn household_food_worker_limit(&self, household_id: BuildingId) -> usize {
        let Some(household) = self.household_by_id(household_id) else {
            return 0;
        };
        if household.food_crisis_level == 0 {
            return 0;
        }
        // Crise nível 2+: todos os membros podem ajudar com comida
        if household.food_crisis_level >= 2 {
            return household.member_ids.len();
        }
        // Crise nível 1: até metade dos membros (mínimo 1)
        let max_workers = if household.member_ids.len() > 2 {
            (household.member_ids.len() + 1) / 2
        } else {
            1
        };
        max_workers.min(household.member_ids.len())
    }

    fn household_assigned_food_support_workers(&self, household_id: BuildingId) -> usize {
        let member_ids = self
            .household_by_id(household_id)
            .map(|household| household.member_ids.clone())
            .unwrap_or_default();
        self.economic_tasks
            .iter()
            .filter(|task| {
                task.actor_household_id == household_id
                    && task.class.is_food_support()
                    && task.phase != EconomicTaskPhase::Completed
                    && task.phase != EconomicTaskPhase::Failed
                    && task
                        .assigned_agent_id
                        .map(|agent_id| member_ids.contains(&agent_id))
                        .unwrap_or(false)
            })
            .count()
    }

    fn allow_food_support_assignment(
        &self,
        household_id: BuildingId,
        agent_id: u64,
        task: &EconomicTask,
    ) -> bool {
        if !task.class.is_food_support() {
            return true;
        }
        if task.assigned_agent_id == Some(agent_id) {
            return true;
        }
        self.household_assigned_food_support_workers(household_id)
            < self.household_food_worker_limit(household_id)
    }

    fn next_task_id(&mut self) -> EconomicTaskId {
        let task_id = self.next_economic_task_id;
        self.next_economic_task_id += 1;
        task_id
    }

    fn ensure_household_food_tasks(&mut self) {
        let households = self.households.clone();
        for household in households {
            let food_units = Self::total_food_units(&household.pantry)
                + Self::total_food_units(&household.reserved_food);
            if food_units >= household.minimum_food_units {
                continue;
            }
            let destination = EconomicNode::HouseholdPantry(household.id);
            let max_purchase_tasks = self.household_food_worker_limit(household.id).max(1);
            let existing_purchase_tasks = self.open_task_count_for_household_class(
                household.id,
                EconomicTaskClass::HouseholdFoodPurchase,
            );
            if existing_purchase_tasks >= max_purchase_tasks {
                continue;
            }
            let mut remaining_deficit = (household.minimum_food_units - food_units).max(0);
            let slots_to_fill = max_purchase_tasks.saturating_sub(existing_purchase_tasks);
            for _ in 0..slots_to_fill {
                if remaining_deficit <= 0 {
                    break;
                }
                let Some((establishment_id, resource, unit_price)) =
                    self.best_food_source_for_household(household.id)
                else {
                    break;
                };
                let amount = remaining_deficit.clamp(2, DEFAULT_CARRYING_CAPACITY);
                let task_id = self.next_task_id();
                let (source, related_establishment_id) = match establishment_id {
                    Some(id) => (EconomicNode::Establishment(id), Some(id)),
                    None => continue,
                };
                self.economic_tasks.push(EconomicTask {
                    id: task_id,
                    kind: EconomicTaskKind::Comprar,
                    class: EconomicTaskClass::HouseholdFoodPurchase,
                    priority: match self.local_norms.rationing_policy {
                        RationingPolicy::HouseholdFirst => 100,
                        RationingPolicy::ProducersFirst => {
                            90u8.saturating_sub((2 - household.food_crisis_level.min(2)) * 10)
                        }
                        _ => 100u8.saturating_sub((2 - household.food_crisis_level.min(2)) * 10),
                    },
                    lock_until_complete: true,
                    creates_household_reserve: resource == ResourceKind::Graos.id(),
                    actor_household_id: household.id,
                    assigned_agent_id: None,
                    source,
                    destination: destination.clone(),
                    resource_id: Some(resource.clone()),
                    amount,
                    unit_price,
                    total_price: unit_price * amount,
                    description: format!(
                        "Comprar {} x{} para {}",
                        self.resource_display_name(&resource),
                        amount,
                        household.name
                    ),
                    phase: EconomicTaskPhase::AwaitingPickup,
                    related_establishment_id,
                });
                remaining_deficit -= amount;
            }
        }
    }

    fn ensure_local_production_tasks(&mut self) {
        let establishments = self.establishments.clone();
        for establishment in establishments {
            let Some(recipe) = self.recipe_for_establishment(&establishment).cloned() else {
                continue;
            };
            let resource_id = recipe.output_resource_id.clone();
            let target = self.stock_target_amount(&establishment, &resource_id);
            let current = Self::total_resource_amount(&establishment.stock, &resource_id);
            if current >= target {
                continue;
            }
            let Some(actor_household_id) = establishment.owner_household_ids.first().copied()
            else {
                continue;
            };
            let destination = EconomicNode::Establishment(establishment.id);
            if self.has_open_task_for(
                actor_household_id,
                EconomicTaskKind::Produzir,
                Some(&resource_id),
                &destination,
            ) {
                continue;
            }
            let amount = recipe
                .output_amount
                .clamp(1, DEFAULT_CARRYING_CAPACITY.max(1));
            let is_food = self.is_food_resource(&resource_id);
            let class = if is_food {
                EconomicTaskClass::FoodProduction
            } else {
                EconomicTaskClass::EssentialProduction
            };
            let priority = if is_food && self.village_food_pressure() > 0 {
                let boost = if self.local_norms.rationing_policy == RationingPolicy::ProducersFirst
                {
                    25
                } else {
                    15
                };
                recipe.priority.saturating_add(boost)
            } else {
                recipe.priority
            };
            let task_id = self.next_task_id();
            self.economic_tasks.push(EconomicTask {
                id: task_id,
                kind: EconomicTaskKind::Produzir,
                class,
                priority,
                lock_until_complete: true,
                creates_household_reserve: false,
                actor_household_id,
                assigned_agent_id: None,
                source: destination.clone(),
                destination,
                resource_id: Some(resource_id.clone()),
                amount,
                unit_price: 0,
                total_price: 0,
                description: format!(
                    "Produzir {} em {}",
                    self.resource_display_name(&resource_id),
                    establishment.name
                ),
                phase: EconomicTaskPhase::AwaitingPickup,
                related_establishment_id: Some(establishment.id),
            });
        }
    }

    fn ensure_establishment_supply_tasks(&mut self) {
        let establishments = self.establishments.clone();
        let village_food_pressure = self.village_food_pressure();
        for establishment in establishments {
            let Some(recipe) = self.recipe_for_establishment(&establishment).cloned() else {
                continue;
            };
            for input in recipe
                .inputs
                .iter()
                .chain(recipe.capital_requirements.iter())
            {
                let priority = if self.is_food_resource(&recipe.output_resource_id) {
                    if self.local_norms.rationing_policy == RationingPolicy::ProducersFirst {
                        100
                    } else if village_food_pressure > 0 {
                        95
                    } else {
                        85
                    }
                } else {
                    recipe.priority.saturating_sub(10).max(35)
                };
                let class = if self.is_food_resource(&recipe.output_resource_id) {
                    EconomicTaskClass::FoodSupplyTransport
                } else {
                    EconomicTaskClass::EssentialProduction
                };
                let max_open_tasks = if self.is_food_resource(&recipe.output_resource_id)
                    && village_food_pressure > 0
                {
                    2
                } else {
                    1
                };
                if let Some(source_establishment_type_id) = self
                    .catalog
                    .recipes
                    .iter()
                    .find(|candidate| candidate.output_resource_id == input.resource_id)
                    .map(|candidate| candidate.establishment_type_id.clone())
                {
                    self.ensure_purchase_shortage_task(
                        &establishment,
                        &input.resource_id,
                        &source_establishment_type_id,
                        input.amount.max(1),
                        class,
                        priority,
                        max_open_tasks,
                    );
                }
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
                Some(ResourceKind::Moedas.id()),
                &destination,
            ) {
                continue;
            }
            let total_amount: i32 = household
                .pending_payments
                .iter()
                .map(|claim| claim.amount)
                .sum();
            let task_id = self.next_task_id();
            self.economic_tasks.push(EconomicTask {
                id: task_id,
                kind: EconomicTaskKind::ReceberPagamento,
                class: EconomicTaskClass::PaymentCollection,
                priority: 88,
                lock_until_complete: true,
                creates_household_reserve: false,
                actor_household_id: household.id,
                assigned_agent_id: None,
                source: EconomicNode::PublicTreasury,
                destination,
                resource_id: Some(ResourceKind::Moedas.id().to_string()),
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
            let Some(recipe) = self.recipe_for_establishment(&establishment).cloned() else {
                continue;
            };
            let primary_output = recipe.output_resource_id;
            let target = self.stock_target_amount(&establishment, &primary_output);
            let current = Self::total_resource_amount(&establishment.stock, &primary_output);
            if current <= target + 3 {
                continue;
            }
            if self.village_food_pressure() > 0 && self.is_food_resource(&primary_output) {
                if self.tick_of_day == 0 {
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: establishment
                            .owner_household_ids
                            .first()
                            .and_then(|id| self.household_by_id(*id))
                            .and_then(|household| household.member_ids.first().copied())
                            .unwrap_or(0),
                        target: None,
                        kind: EventKind::Scarcity,
                        summary: format!(
                            "Venda de excedente de {} em {} foi bloqueada por pressao alimentar da vila.",
                            self.resource_display_name(&primary_output),
                            establishment.name
                        ),
                        impact_tags: vec!["venda_bloqueada".to_string(), "escassez".to_string()],
                    });
                }
                continue;
            }
            if self.has_open_task_for(
                establishment
                    .owner_household_ids
                    .first()
                    .copied()
                    .unwrap_or_default(),
                EconomicTaskKind::Vender,
                Some(&primary_output),
                &EconomicNode::ExternalMarket,
            ) {
                continue;
            }
            if let Some(actor_household_id) = establishment.owner_household_ids.first().copied() {
                let resource_def = self.resource_def(&primary_output);
                if !resource_def
                    .map(|resource| resource.can_sell_external)
                    .unwrap_or(false)
                {
                    continue;
                }
                let unit_price = self
                    .market_quote(&primary_output)
                    .map(|quote| quote.sell_price)
                    .unwrap_or(self.base_price(&primary_output));
                let task_id = self.next_task_id();
                self.economic_tasks.push(EconomicTask {
                    id: task_id,
                    kind: EconomicTaskKind::Vender,
                    class: EconomicTaskClass::SurplusSale,
                    priority: 20,
                    lock_until_complete: true,
                    creates_household_reserve: false,
                    actor_household_id,
                    assigned_agent_id: None,
                    source: EconomicNode::Establishment(establishment.id),
                    destination: EconomicNode::ExternalMarket,
                    resource_id: Some(primary_output.clone()),
                    amount: 2,
                    unit_price,
                    total_price: unit_price * 2,
                    description: format!(
                        "Vender excedente de {} em {}",
                        self.resource_display_name(&primary_output),
                        establishment.name
                    ),
                    phase: EconomicTaskPhase::AwaitingPickup,
                    related_establishment_id: Some(establishment.id),
                });
            }
        }
    }

    fn best_food_source_for_household(
        &self,
        household_id: BuildingId,
    ) -> Option<(Option<EstablishmentId>, String, i32)> {
        let dest_village_idx = self.village_index_of_household(household_id)?;
        let food_order = self.food_resource_ids_sorted();
        let mut offers = self
            .establishments
            .iter()
            .filter_map(|establishment| {
                let best_stock = food_order
                    .iter()
                    .into_iter()
                    .find(|resource_id| {
                        Self::total_resource_amount(&establishment.stock, resource_id.as_str()) > 0
                    })?
                    .clone();
                let unit_price = establishment
                    .posted_prices
                    .iter()
                    .find(|price| price.resource_id == best_stock)
                    .map(|price| price.unit_price)
                    .unwrap_or_else(|| self.base_price(&best_stock));
                
                let is_local = self.village_index_of_establishment(establishment.id) == Some(dest_village_idx);
                let final_price = if is_local {
                    unit_price
                } else {
                    (unit_price as f64 * 1.3) as i32
                };
                Some((Some(establishment.id), best_stock, final_price, is_local))
            })
            .collect::<Vec<_>>();

        offers.sort_by_key(|(_, resource_id, price, is_local)| {
            (!is_local, *price, resource_id.clone())
        });

        let treasury = self
            .household_by_id(household_id)
            .map(|household| household.treasury)
            .unwrap_or(0);

        offers
            .into_iter()
            .filter(|(_, _, price, _)| treasury >= *price)
            .map(|(est_id, resource_id, price, _)| (est_id, resource_id, price))
            .next()
    }

    fn ensure_purchase_shortage_task(
        &mut self,
        destination_establishment: &EstablishmentEconomy,
        resource_id: &str,
        source_establishment_type_id: &str,
        amount: i32,
        class: EconomicTaskClass,
        priority: u8,
        max_open_tasks: usize,
    ) {
        let current = Self::total_resource_amount(&destination_establishment.stock, resource_id);
        let target = self.stock_target_amount(destination_establishment, resource_id);
        if current >= target {
            return;
        }
        let Some(actor_household_id) = destination_establishment
            .owner_household_ids
            .first()
            .copied()
        else {
            return;
        };

        // Find the buyer's village index
        let Some(dest_village_idx) = self.village_index_of_establishment(destination_establishment.id) else {
            return;
        };

        // Find candidate supplier establishments that have enough stock
        let mut candidates = self.establishments
            .iter()
            .filter(|candidate| {
                candidate.establishment_type_id == source_establishment_type_id
                    && Self::total_resource_amount(&candidate.stock, resource_id) >= amount
            })
            .cloned()
            .collect::<Vec<_>>();

        // Sort candidates so that we prioritize:
        // 1. Same village (dest_village_idx)
        // 2. Lowest total cost (including 30% inter-village import tax if different village)
        let get_total_cost = |candidate: &EstablishmentEconomy| -> i32 {
            let unit_price = candidate
                .posted_prices
                .iter()
                .find(|price| price.resource_id == resource_id)
                .map(|price| price.unit_price)
                .unwrap_or_else(|| self.base_price(resource_id));
            let is_local = self.village_index_of_establishment(candidate.id) == Some(dest_village_idx);
            if is_local {
                unit_price
            } else {
                (unit_price as f64 * 1.3) as i32
            }
        };

        candidates.sort_by_key(|c| {
            let is_local = self.village_index_of_establishment(c.id) == Some(dest_village_idx);
            let cost = get_total_cost(c);
            (!is_local, cost)
        });

        let Some(best_supplier) = candidates.first() else {
            // No supplier has enough stock in the entire world.
            // Log global scarcity and do not generate any task.
            let owner_agent_id = self.household_by_id(actor_household_id)
                .and_then(|h| h.member_ids.first().copied())
                .unwrap_or(0);
            
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: owner_agent_id,
                target: Some(destination_establishment.id),
                kind: EventKind::Scarcity,
                summary: format!(
                    "Producao de {} paralisada: falta do insumo {} em todas as vilas.",
                    destination_establishment.name,
                    self.resource_display_name(resource_id)
                ),
                impact_tags: vec!["escassez".to_string(), "producao_parada".to_string(), resource_id.to_string()],
            });
            return;
        };

        // Create Comprar task
        let destination = EconomicNode::Establishment(destination_establishment.id);
        let source_node = EconomicNode::Establishment(best_supplier.id);

        let existing = self.matching_open_task_count(
            actor_household_id,
            EconomicTaskKind::Comprar,
            Some(resource_id),
            &destination,
            Some(&source_node),
        );
        if existing >= max_open_tasks {
            return;
        }

        let unit_price = get_total_cost(best_supplier);
        let desired_amount = amount.clamp(1, DEFAULT_CARRYING_CAPACITY);
        let total_price = unit_price * desired_amount;

        for _ in existing..max_open_tasks {
            let task_id = self.next_task_id();
            let is_local = self.village_index_of_establishment(best_supplier.id) == Some(dest_village_idx);
            let description = if is_local {
                format!(
                    "Comprar {} x{} de {} para {}",
                    self.resource_display_name(resource_id),
                    desired_amount,
                    best_supplier.name,
                    destination_establishment.name
                )
            } else {
                format!(
                    "Importar {} x{} de {} para {} (taxa inter-vilas)",
                    self.resource_display_name(resource_id),
                    desired_amount,
                    best_supplier.name,
                    destination_establishment.name
                )
            };

            self.economic_tasks.push(EconomicTask {
                id: task_id,
                kind: EconomicTaskKind::Comprar,
                class,
                priority,
                lock_until_complete: true,
                creates_household_reserve: false,
                actor_household_id,
                assigned_agent_id: None,
                source: source_node.clone(),
                destination: destination.clone(),
                resource_id: Some(resource_id.to_string()),
                amount: desired_amount,
                unit_price,
                total_price,
                description,
                phase: EconomicTaskPhase::AwaitingPickup,
                related_establishment_id: Some(destination_establishment.id),
            });
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

    fn compute_chaos_pressure(
        state: &AgentState,
        profile: &AgentProfile,
        relations: &HashMap<u64, AgentRelation>,
        injury: &InjuryState,
        household_treasury: i32,
        food_crisis_level: u8,
    ) -> u32 {
        let max_resentment = relations.values().map(|r| r.resentment).max().unwrap_or(0);
        let trauma_count = profile.trauma_traits.len() as i32;

        let raw = (state.stress as f64 * 0.3)
            + (state.hunger as f64 * 0.25)
            + (max_resentment as f64 * 0.2)
            + (trauma_count as f64 * 8.0)
            + (injury.pain as f64 * 0.15)
            + if household_treasury <= 0 { 15.0 } else { 0.0 }
            + if food_crisis_level >= 2 { 10.0 } else { 0.0 };

        (raw as u32).min(100)
    }

    fn apply_trauma_trait(&mut self, agent_id: u64, trait_name: &str) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut profile = entity_mut
            .get_mut::<ProfileComponent>()
            .ok_or_else(|| anyhow!("missing profile component"))?;
        let trait_str = trait_name.to_string();
        if !profile.0.trauma_traits.contains(&trait_str) {
            profile.0.trauma_traits.push(trait_str);
        }
        Ok(())
    }

    fn apply_trauma_traits_for_event(
        &mut self,
        agent_id: u64,
        role: &str,
        event_kind: EventKind,
    ) -> Result<()> {
        match (event_kind, role) {
            (EventKind::Violence, "victim") => {
                self.apply_trauma_trait(agent_id, "traumatizado")?;
                self.apply_trauma_trait(agent_id, "vingativo")?;
            }
            (EventKind::Violence, "witness_first") => {
                self.apply_trauma_trait(agent_id, "assustado")?;
            }
            (EventKind::Violence, "witness_repeat") => {
                self.apply_trauma_trait(agent_id, "insensibilizado")?;
            }
            (EventKind::Theft, "victim") => {
                self.apply_trauma_trait(agent_id, "desconfiado")?;
                self.apply_trauma_trait(agent_id, "vingativo")?;
            }
            (EventKind::Death, "witness") => {
                self.apply_trauma_trait(agent_id, "traumatizado")?;
                self.apply_trauma_trait(agent_id, "nihilista")?;
            }
            (EventKind::Punishment, "victim") => {
                self.apply_trauma_trait(agent_id, "ressentido")?;
                self.apply_trauma_trait(agent_id, "rebelde")?;
            }
            _ => {}
        }
        Ok(())
    }

    fn propagate_witness_effects(
        &mut self,
        event_building_id: Option<BuildingId>,
        aggressor_id: u64,
        victim_id: u64,
        event_kind: EventKind,
    ) -> Result<()> {
        let Some(building_id) = event_building_id else {
            return Ok(());
        };
        // Collect witness IDs (agents in same building, excluding aggressor and victim)
        let witness_ids: Vec<u64> = {
            let mut query = self.world.query::<(&AgentCore, &PositionComponent, &LifeStatusComponent)>();
            query
                .iter(&self.world)
                .filter(|(core, _, life)| {
                    core.id != aggressor_id
                        && core.id != victim_id
                        && life.0 == AgentLifeStatus::Vivo
                })
                .filter(|(_, pos, _)| {
                    self.tile_at(pos.0)
                        .and_then(|t| t.building_id)
                        .map(|bid| bid == building_id)
                        .unwrap_or(false)
                })
                .map(|(core, _, _)| core.id)
                .collect()
        };

        let aggressor_name = self.agent_name(aggressor_id).unwrap_or_default();
        let victim_name = self.agent_name(victim_id).unwrap_or_default();
        let event_desc = match event_kind {
            EventKind::Violence => "agredir",
            EventKind::Theft => "roubar",
            EventKind::Death => "matar",
            _ => "atacar",
        };

        for witness_id in witness_ids {
            // 1. Add memory
            self.add_memory(
                witness_id,
                MemoryKind::Impression,
                format!("Presenciou {} {} {}.", aggressor_name, event_desc, victim_name),
                vec!["violencia".to_string(), "testemunha".to_string()],
                25,
                vec![aggressor_id, victim_id],
            )?;

            // 2. Increase stress
            {
                let entity = self.find_agent_entity(witness_id)?;
                let mut entity_mut = self.world.entity_mut(entity);
                if let Some(mut state) = entity_mut.get_mut::<StateComponent>() {
                    state.0.stress = (state.0.stress + 10).clamp(0, 100);
                }
            }

            // 3. Increase resentment against aggressor
            self.apply_relation_delta(
                witness_id,
                aggressor_id,
                &RelationDelta {
                    trust: -3,
                    friendship: -2,
                    resentment: 5,
                    attraction: 0,
                    moral_debt: 0,
                    reputation: -3,
                },
            )?;

            // 4. Check violence_witnessed_count for trait assignment
            let witnessed_before = {
                let entity = self.find_agent_entity(witness_id)?;
                let entry = self.world.entity(entity);
                entry
                    .get::<TraumaTrackerComponent>()
                    .map(|t| t.0.violence_witnessed_count)
                    .unwrap_or(0)
            };

            // Increment witness count
            {
                let entity = self.find_agent_entity(witness_id)?;
                let mut entity_mut = self.world.entity_mut(entity);
                if let Some(mut tracker) = entity_mut.get_mut::<TraumaTrackerComponent>() {
                    tracker.0.violence_witnessed_count += 1;
                }
            }

            if witnessed_before >= 1 {
                self.apply_trauma_traits_for_event(witness_id, "witness_repeat", event_kind)?;
            } else {
                self.apply_trauma_traits_for_event(witness_id, "witness_first", event_kind)?;
            }
        }
        Ok(())
    }

    fn update_trauma_trackers(&mut self) -> Result<()> {
        // Collect agent data needed for continuous tracking
        let agent_data: Vec<(u64, i32, i32, i32)> = {
            let mut query = self.world.query::<(
                &AgentCore,
                &StateComponent,
                &LifeStatusComponent,
            )>();
            query
                .iter(&self.world)
                .filter(|(_, _, life)| life.0 == AgentLifeStatus::Vivo)
                .map(|(core, state, _)| {
                    (core.id, state.0.hunger, state.0.stress, 0i32)
                })
                .collect()
        };

        // Get household treasury for each agent
        let agent_treasury: HashMap<u64, i32> = agent_data
            .iter()
            .filter_map(|(id, _, _, _)| {
                let hh_id = self.household_id_for_agent(*id)?;
                let treasury = self.household_by_id(hh_id).map(|h| h.treasury).unwrap_or(0);
                Some((*id, treasury))
            })
            .collect();

        for (agent_id, hunger, stress, _) in &agent_data {
            let treasury = agent_treasury.get(agent_id).copied().unwrap_or(0);

            let entity = self.find_agent_entity(*agent_id)?;
            let mut entity_mut = self.world.entity_mut(entity);
            if let Some(mut tracker) = entity_mut.get_mut::<TraumaTrackerComponent>() {
                // Track consecutive starvation
                if *hunger >= 90 {
                    tracker.0.consecutive_starving_ticks += 1;
                } else {
                    tracker.0.consecutive_starving_ticks = 0;
                }
                // Track consecutive high stress
                if *stress >= 85 {
                    tracker.0.consecutive_stressed_ticks += 1;
                } else {
                    tracker.0.consecutive_stressed_ticks = 0;
                }
                // Track consecutive wealth
                if treasury >= 500 {
                    tracker.0.consecutive_wealthy_ticks += 1;
                } else {
                    tracker.0.consecutive_wealthy_ticks = 0;
                }
            }
        }

        // Check thresholds and apply traits (3 days = 3 * ticks_per_day)
        let three_days = self.ticks_per_day * 3;
        let two_days = self.ticks_per_day * 2;
        let five_days = self.ticks_per_day * 5;

        let trackers: Vec<(u64, u32, u32, u32)> = {
            let mut query = self.world.query::<(&AgentCore, &TraumaTrackerComponent)>();
            query
                .iter(&self.world)
                .map(|(core, tracker)| {
                    (
                        core.id,
                        tracker.0.consecutive_starving_ticks,
                        tracker.0.consecutive_stressed_ticks,
                        tracker.0.consecutive_wealthy_ticks,
                    )
                })
                .collect()
        };

        for (agent_id, starving, stressed, wealthy) in trackers {
            if starving == three_days {
                self.apply_trauma_trait(agent_id, "desesperado")?;
                self.apply_trauma_trait(agent_id, "impulsivo")?;
            }
            if stressed == two_days {
                self.apply_trauma_trait(agent_id, "instavel")?;
                self.apply_trauma_trait(agent_id, "paranoico")?;
            }
            if wealthy == five_days {
                self.apply_trauma_trait(agent_id, "ganancioso")?;
                self.apply_trauma_trait(agent_id, "arrogante")?;
            }
        }

        Ok(())
    }
}


#[allow(dead_code)]
#[derive(Clone)]
struct AgentContext {
    id: u64,
    name: String,
    role_id: String,
    position: TileCoord,
    state: AgentState,
    life_status: AgentLifeStatus,
    profile: AgentProfile,
    relations: HashMap<u64, AgentRelation>,
    memories: Vec<AgentMemory>,
    destination_label: Option<String>,
    current_building_id: Option<BuildingId>,
    current_room_id: Option<RoomId>,
    last_intent: Option<AgentIntent>,
    llm_calls: u64,
    blocked_ticks: u32,
    active_conversation_id: Option<ConversationId>,
    social_cooldown_until: u64,
    household_id: Option<BuildingId>,
    task_queue: VecDeque<SimplifiedTask>,
    trauma_tracker: TraumaTracker,
}

struct PreparedDecisionRequest {
    agent_id: u64,
    nearby_ids: Vec<u64>,
    cognition_trigger: String,
    social_opportunity_signature: Option<String>,
    input: DecisionInput,
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



fn merge_stack(stacks: &mut Vec<ResourceStack>, stack: ResourceStack) {
    if let Some(existing) = stacks
        .iter_mut()
        .find(|existing| existing.resource_id == stack.resource_id)
    {
        existing.amount += stack.amount;
    } else {
        stacks.push(stack);
    }
}

fn consume_matching(stacks: &mut Vec<ResourceStack>, accepted: &[&str]) -> bool {
    for stack in stacks.iter_mut() {
        if accepted.contains(&stack.resource_id.as_str()) && stack.amount > 0 {
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

fn sentence_for_case_severity(justice_severity: JusticeSeverity, severity: u8) -> SentenceKind {
    match justice_severity {
        JusticeSeverity::Lenient => {
            if severity >= 90 {
                SentenceKind::Fine
            } else {
                SentenceKind::Restitution
            }
        }
        JusticeSeverity::Normal => {
            if severity >= 90 {
                SentenceKind::Detention
            } else if severity >= 60 {
                SentenceKind::Fine
            } else {
                SentenceKind::Restitution
            }
        }
        JusticeSeverity::Severe => {
            if severity >= 80 {
                SentenceKind::Detention
            } else if severity >= 45 {
                SentenceKind::Fine
            } else {
                SentenceKind::Restitution
            }
        }
    }
}

fn political_issue_summary(domain: PolicyDomain, proposed_value: &str, agenda_tag: &str) -> String {
    match domain {
        PolicyDomain::Tax => match proposed_value {
            "reduzir" => "reduzir o imposto diario por lar".to_string(),
            "aumentar" => "aumentar o imposto diario para sustentar o caixa publico".to_string(),
            _ => format!("alterar imposto: {agenda_tag}"),
        },
        PolicyDomain::Justice => match proposed_value {
            "branda" => "abrandar a severidade das punicoes locais".to_string(),
            "severa" => "endurecer a resposta judicial local".to_string(),
            _ => format!("alterar justica: {agenda_tag}"),
        },
        PolicyDomain::Rationing => match proposed_value {
            "lares" => "priorizar lares famintos no racionamento alimentar".to_string(),
            "produtores" => "priorizar produtores de comida no racionamento alimentar".to_string(),
            "civico" => "priorizar estabilidade civica no racionamento".to_string(),
            _ => format!("alterar racionamento: {agenda_tag}"),
        },
    }
}

fn faction_name(domain: PolicyDomain, proposed_value: &str) -> String {
    match (domain, proposed_value) {
        (PolicyDomain::Tax, "reduzir") => "Liga contra o Imposto".to_string(),
        (PolicyDomain::Tax, "aumentar") => "Partidarios do Caixa Publico".to_string(),
        (PolicyDomain::Justice, "branda") => "Defensores da Clemencia".to_string(),
        (PolicyDomain::Justice, "severa") => "Defensores da Ordem".to_string(),
        (PolicyDomain::Rationing, "lares") => "Coalizao das Despensas".to_string(),
        (PolicyDomain::Rationing, "produtores") => "Coalizao dos Fornos e Campos".to_string(),
        (PolicyDomain::Rationing, "civico") => "Coalizao Civica".to_string(),
        _ => format!("Faccao de {}", domain.as_str()),
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

