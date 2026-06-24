use crate::agent_mind::{
    ConversationContextInput, ConversationObservedAgentInput, ConversationTurnInput, DecisionInput,
    EconomicContextInput, EconomicOpportunityInput, FeudalContextInput, InformationContextInput,
    InstitutionalContextInput, LegalContextInput, MeetingResponse, NearbyAgentInput,
    NearbyFixtureInput, PoliticalContextInput, ProposedMeeting, PsychologicalContextInput,
    RecentEventInput, RelationalHistoryInput, ThinkMakerInput, ThinkMakerOutput, TimeContextInput,
    WorldPlaceInput, parse_action_planner_output, retrieve_relational_memories,
    retrieve_relevant_memories, validate_intent,
};
use crate::economy_catalog::{default_economy_catalog, validate_catalog};
use crate::llm_adapter::{LlmAdapter, LlmError};
use crate::world_model::{
    ActivePromise, AgentIntent, AgentLifeStatus, AgentMemory, AgentProfile, AgentRelation,
    AgentSnapshot, AgentState, AuthorityOffice, AuthorityOfficeId, BuildingId, BuildingSpec,
    CaravanState, CombatId, CombatOutcome, CombatState, CombatStatus, ConstructionProject,
    ConstructionStatus, ConversationId, ConversationOutcome, ConversationParticipantState,
    ConversationState, ConversationStatus, ConversationTurn, CopingPattern, CopingPatternKind,
    CraftProficiencyState, Creature, CrimeCase, CrimeCaseId, CrimeCaseStatus, CrimeType, CropStage,
    CropState, CulturalStory, CulturalStoryId, CulturalStoryKind, CulturalTradition, EconomicNode,
    EconomicTask, EconomicTaskClass, EconomicTaskId, EconomicTaskKind, EconomicTaskPhase,
    EconomyCatalog, EquipmentSlot, EscrowAccount, EstablishmentEconomy, EstablishmentId,
    EstateHolding, EstateHoldingId, EventKind, FactionObjective, FeudalContract, FeudalContractId,
    FeudalContractStatus, FeudalRank, FeudalTitle, FeudalTitleId, FixtureId, FixtureKind,
    FixtureSpec, ForeignRelation, ForeignRelationId, HistoricalBootstrapSummary, HouseholdEconomy,
    HuntingQuest, InjuryState, InnerContradiction, InstitutionalPerception, InsurrectionId,
    InsurrectionStage, InsurrectionState, InsurrectionStatus, IntentKind, ItemAffordanceKind,
    ItemClass, ItemInstance, ItemInstanceId, JusticeSeverity, LocalNorms, LocationKind, MemoryKind,
    MilitaryDemand, MilitaryDemandId, MilitaryDemandStatus, PendingPaymentClaim, PersonalSymbol,
    PersonalSymbolTargetKind, PolicyAct, PolicyActId, PolicyActStatus, PolicyAuthority,
    PolicyDomain, PolicyEffect, PolicyFavor, PolicyScope, PolicyTarget, PoliticalFaction,
    PoliticalFactionId, PoliticalIssue, PoliticalIssueId, PoliticalIssueStatus, PoliticalPressure,
    Polity, PolityId, PostedPrice, PowerCenter, PowerCenterId, PromiseCondition,
    PsychologicalState, RationingPolicy, RefinementLevel, RelationDelta, ResourceKind,
    ResourceStack, Role, RoomId, RoomSpec, Rumor, RumorBelief, SNAPSHOT_SCHEMA_VERSION,
    ScarcityMetric, ScheduledMeeting, ScheduledMeetingId, ScheduledMeetingStatus, Secret,
    SecretKind, SentenceKind, SimplifiedTask, SimulationSnapshot, SocialMove, SpatialSnapshot,
    StoryBelief, StoryStatus, StoryVersion, SuccessionCrisis, SuccessionCrisisId,
    SuccessionCrisisStatus, Territory, TerritoryId, TileCoord, TileKind, TileSpec, TraumaTracker,
    VillageEconomy, WarId, WarStage, WarState, WarStatus, WorldEvent, WorldPlaceKind,
    WorldPlaceRef,
};
use anyhow::{Result, anyhow};
use bevy_ecs::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

mod cognition;
mod conflict;
mod debug;
mod economy;
mod fauna;
mod helpers;
mod navigation;
mod politics;
mod social;
mod tick;
mod utility_ai;
mod views;

use cognition::{AgentContext, ConversationBatchItem, PreparedConversationTurn};
use helpers::{
    consume_matching, extend_summary, invert_delta, merge_stack, other_participant,
    political_issue_summary, reconstruct_path, sentence_for_case_severity, social_goal_from_move,
};
pub use tick::tick_interval_ms;
use utility_ai::UtilityControlComponent;

pub const SIMULATED_MINUTES_PER_TICK: u32 = 1;
pub const DEFAULT_TICKS_PER_DAY: u32 = 24 * 60 / SIMULATED_MINUTES_PER_TICK;
pub const DEFAULT_TICKS_PER_SECOND: u32 = 1;
pub const MAX_TICKS_PER_SECOND: u32 = 10;
const MAX_CONVERSATION_TURNS: u32 = 6;
const CONVERSATION_RECENT_TURNS_LIMIT: usize = 6;
const ROUTINE_RECONSIDERATION_MAX: u32 = 4;

const BLOCKED_RECONSIDERATION_TICKS: u32 = 2;
const DEFAULT_CARRYING_CAPACITY: i32 = 4;

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
pub struct InstitutionalPerceptionComponent(pub InstitutionalPerception);

#[derive(Component, Clone, Default)]
pub struct PsychologicalStateComponent(pub PsychologicalState);

#[derive(Component, Clone, Default)]
pub struct RumorBeliefComponent(pub Vec<RumorBelief>);

#[derive(Component, Clone, Default)]
pub struct StoryBeliefComponent(pub Vec<StoryBelief>);

#[derive(Component, Clone, Default)]
pub struct RelationComponent(pub HashMap<u64, AgentRelation>);

#[derive(Component, Clone, Default)]
pub struct MemoryComponent(pub Vec<AgentMemory>);

#[derive(Component, Clone, Default)]
pub struct InventoryComponent(pub Vec<ResourceStack>);

#[derive(Component, Clone, Default)]
pub struct ItemInventoryComponent(pub Vec<ItemInstanceId>);

#[derive(Component, Clone, Default)]
pub struct EquipmentComponent(pub HashMap<EquipmentSlot, ItemInstanceId>);

#[derive(Component, Clone, Default)]
pub struct CraftProficiencyComponent(pub CraftProficiencyState);

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

#[derive(Debug)]
pub struct CompletedActionPlan {
    pub agent_id: u64,
    pub nearby_ids: Vec<u64>,
    pub cognition_trigger: String,
    pub social_opportunity_signature: Option<String>,
    pub input: DecisionInput,
    pub raw_plan: String,
}

#[derive(Debug)]
pub struct SkippedActionPlan {
    pub agent_id: u64,
    pub cognition_trigger: String,
    pub social_opportunity_signature: Option<String>,
    pub error: LlmError,
}

#[derive(Debug)]
pub enum ActionPlannerResult {
    Completed(CompletedActionPlan),
    Skipped(SkippedActionPlan),
}

#[derive(Component, Clone, Default)]
pub struct LineageComponent {
    pub age: u32,
    pub parents: Vec<u64>,
    pub children: Vec<u64>,
    pub spouse: Option<u64>,
    pub gender: String,
    pub mourning_days_left: u32,
}

#[derive(Component, Clone, Default)]
pub struct ConversationComponent {
    pub active_conversation_id: Option<ConversationId>,
    pub conversation_participant_ids: Vec<u64>,
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

#[derive(Component, Clone)]
pub struct CreatureCore {
    pub id: u64,
    pub name: String,
    pub species: String,
    pub is_legendary: bool,
    pub habitat_territory_id: u64,
}

#[derive(Component, Clone)]
pub struct CreatureStateComponent {
    pub health: i32,
    pub max_health: i32,
    pub attack_power: i32,
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
    pub num_villages: usize,
    pub history_years: u32,
    pub history_founding_households: usize,
    pub history_seed: Option<u64>,
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
            history_years: 100,
            history_founding_households: 3,
            history_seed: None,
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
    pub institutional_perception: InstitutionalPerception,
    pub psychological_state: PsychologicalState,
    pub craft_proficiencies: CraftProficiencyState,
    pub perceived_status_score: i32,
    pub visible_prestige_summary: String,
    pub equipped_items: Vec<String>,
    pub inventory_items: Vec<String>,
    pub rumor_beliefs: Vec<RumorBelief>,
    pub known_stories: Vec<String>,
    pub known_rumors: Vec<String>,
    pub last_intent: Option<AgentIntent>,
    pub last_thought: String,
    pub recent_memories: Vec<AgentMemory>,
    pub relations: Vec<(u64, AgentRelation)>,
    pub active_conversation_id: Option<ConversationId>,
    pub conversation_participant_names: Vec<String>,
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
    pub work_establishment_items: Vec<String>,
    pub local_prices: Vec<PostedPrice>,
    pub public_treasury: i32,
    pub political_position: String,
    pub political_grievances: Vec<String>,
    pub feudal_title: Option<String>,
    pub direct_lord_name: Option<String>,
    pub subordinate_names: Vec<String>,
    pub feudal_obligations: Vec<String>,
    pub feudal_power_summary: Option<String>,
    pub succession_status: Vec<String>,
    pub scheduled_meetings: Vec<String>,
    pub planner_status: String,
    pub active_utility_directive: Option<String>,
    pub reactive_stance: String,
    pub reactive_reason: String,
    pub reactive_revenge_target: Option<String>,
    pub reactive_status_pressure: String,
    pub reactive_defiance_posture: String,
    pub control_mode: String,
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

struct PendingActionPlan {
    agent_id: u64,
    handle: std::thread::JoinHandle<ActionPlannerResult>,
}

pub struct Simulation {
    catalog: EconomyCatalog,
    world: World,
    spatial: SpatialSnapshot,
    village_name: String,
    world_history_years_simulated: u32,
    world_foundation_year: i32,
    historical_summary: Option<HistoricalBootstrapSummary>,
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
    next_policy_act_id: PolicyActId,
    next_territory_id: TerritoryId,
    next_polity_id: PolityId,
    next_foreign_relation_id: ForeignRelationId,
    next_war_id: WarId,
    next_military_demand_id: MilitaryDemandId,
    next_insurrection_id: InsurrectionId,
    next_cultural_story_id: CulturalStoryId,
    next_scheduled_meeting_id: ScheduledMeetingId,
    next_feudal_title_id: FeudalTitleId,
    next_feudal_contract_id: FeudalContractId,
    next_estate_holding_id: EstateHoldingId,
    next_succession_crisis_id: SuccessionCrisisId,
    next_power_center_id: PowerCenterId,
    next_authority_office_id: AuthorityOfficeId,
    next_item_instance_id: ItemInstanceId,
    political_factions: Vec<PoliticalFaction>,
    political_issues: Vec<PoliticalIssue>,
    policy_acts: Vec<PolicyAct>,
    territories: Vec<Territory>,
    polities: Vec<Polity>,
    foreign_relations: Vec<ForeignRelation>,
    wars: Vec<WarState>,
    military_demands: Vec<MilitaryDemand>,
    insurrections: Vec<InsurrectionState>,
    feudal_titles: Vec<FeudalTitle>,
    feudal_contracts: Vec<FeudalContract>,
    estate_holdings: Vec<EstateHolding>,
    succession_crises: Vec<SuccessionCrisis>,
    power_centers: Vec<PowerCenter>,
    authority_offices: Vec<AuthorityOffice>,
    political_pressures: Vec<PoliticalPressure>,
    local_norms: LocalNorms,
    next_economic_task_id: EconomicTaskId,
    next_construction_project_id: u64,
    item_instances: Vec<ItemInstance>,
    households: Vec<HouseholdEconomy>,
    establishments: Vec<EstablishmentEconomy>,
    village_economy: VillageEconomy,
    economic_tasks: Vec<EconomicTask>,
    construction_projects: Vec<ConstructionProject>,
    pending_thoughts: Vec<PendingThoughts>,
    pending_action_plans: Vec<PendingActionPlan>,
    pub crops: HashMap<TileCoord, CropState>,
    pub secrets: Vec<Secret>,
    pub caravans: Vec<CaravanState>,
    next_secret_id: u64,
    next_creature_id: u64,
    next_hunting_quest_id: u64,
    pub hunting_quests: Vec<HuntingQuest>,

    pub promises: Vec<ActivePromise>,
    pub policy_favors: Vec<PolicyFavor>,
    pub rumors: Vec<Rumor>,
    pub cultural_stories: Vec<CulturalStory>,
    pub story_versions: Vec<StoryVersion>,
    pub cultural_traditions: Vec<CulturalTradition>,
    pub scheduled_meetings: Vec<ScheduledMeeting>,
    pub active_escrows: Vec<EscrowAccount>,
    next_rumor_id: u64,
}

impl Drop for Simulation {
    fn drop(&mut self) {
        for pending in self.pending_thoughts.drain(..) {
            let _ = pending.handle.join();
        }
        for pending in self.pending_action_plans.drain(..) {
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
        let policy_acts = snapshot.policy_acts.clone();
        let territories = snapshot.territories.clone();
        let polities = snapshot.polities.clone();
        let foreign_relations = snapshot.foreign_relations.clone();
        let wars = snapshot.wars.clone();
        let military_demands = snapshot.military_demands.clone();
        let insurrections = snapshot.insurrections.clone();
        let feudal_titles = snapshot.feudal_titles.clone();
        let feudal_contracts = snapshot.feudal_contracts.clone();
        let estate_holdings = snapshot.estate_holdings.clone();
        let succession_crises = snapshot.succession_crises.clone();
        let power_centers = snapshot.power_centers.clone();
        let authority_offices = snapshot.authority_offices.clone();
        let scheduled_meetings = snapshot.scheduled_meetings.clone();
        let political_pressures = snapshot.political_pressures.clone();
        let local_norms = snapshot.local_norms.clone();
        for agent in snapshot.agents {
            world.spawn((
                (
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
                    InstitutionalPerceptionComponent(agent.institutional_perception),
                    PsychologicalStateComponent(agent.psychological_state),
                    RumorBeliefComponent(agent.rumor_beliefs),
                    StoryBeliefComponent(agent.story_beliefs),
                ),
                (
                    RelationComponent(agent.relations),
                    LineageComponent {
                        age: agent.age,
                        parents: agent.parents,
                        children: agent.children,
                        spouse: agent.spouse,
                        gender: agent.gender,
                        mourning_days_left: 0,
                    },
                    MemoryComponent(agent.memories),
                    InventoryComponent(agent.inventory),
                    ItemInventoryComponent(agent.inventory_item_ids),
                    EquipmentComponent(agent.equipped_items),
                    CraftProficiencyComponent(agent.craft_proficiencies),
                    PositionComponent(agent.position),
                ),
                (
                    DestinationComponent(agent.destination),
                    DestinationLabelComponent(agent.destination_label),
                    PathComponent(agent.planned_path),
                    IntentComponent(agent.last_intent),
                    TaskQueueComponent(agent.task_queue.into()),
                ),
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
                        conversation_participant_ids: agent.conversation_participant_ids,
                        last_social_act: agent.last_social_act,
                        social_cooldown_until: agent.social_cooldown_until,
                    },
                    EconomicActivityComponent {
                        active_task_id: agent.active_economic_task_id,
                        carrying: agent.carrying,
                        carrying_capacity: agent.carrying_capacity,
                    },
                    TraumaTrackerComponent(agent.trauma_tracker),
                    UtilityControlComponent::default(),
                ),
            ));
        }

        for creature in snapshot.creatures {
            world.spawn((
                CreatureCore {
                    id: creature.id,
                    name: creature.name.clone(),
                    species: creature.species.clone(),
                    is_legendary: creature.is_legendary,
                    habitat_territory_id: creature.habitat_territory_id,
                },
                CreatureStateComponent {
                    health: creature.health,
                    max_health: creature.max_health,
                    attack_power: creature.attack_power,
                },
                InjuryComponent(creature.injury),
                PositionComponent(creature.position),
                LifeStatusComponent(if creature.active {
                    AgentLifeStatus::Vivo
                } else {
                    AgentLifeStatus::Morto
                }),
                DestinationComponent(None),
                PathComponent(Vec::new()),
            ));
        }

        Self {
            catalog,
            world,
            spatial: snapshot.spatial,
            village_name: snapshot.village_name,
            world_history_years_simulated: snapshot.world_history_years_simulated,
            world_foundation_year: snapshot.world_foundation_year,
            historical_summary: snapshot.historical_summary.clone(),
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
            next_policy_act_id: snapshot.next_policy_act_id,
            next_territory_id: snapshot.next_territory_id,
            next_polity_id: snapshot.next_polity_id,
            next_foreign_relation_id: snapshot.next_foreign_relation_id,
            next_war_id: snapshot.next_war_id,
            next_military_demand_id: snapshot.next_military_demand_id,
            next_insurrection_id: snapshot.next_insurrection_id,
            next_scheduled_meeting_id: snapshot.next_scheduled_meeting_id.max(
                scheduled_meetings
                    .iter()
                    .map(|meeting| meeting.id)
                    .max()
                    .unwrap_or(0)
                    + 1,
            ),
            next_feudal_title_id: snapshot.next_feudal_title_id.max(
                feudal_titles
                    .iter()
                    .map(|title| title.id)
                    .max()
                    .unwrap_or(0)
                    + 1,
            ),
            next_feudal_contract_id: snapshot.next_feudal_contract_id.max(
                feudal_contracts
                    .iter()
                    .map(|contract| contract.id)
                    .max()
                    .unwrap_or(0)
                    + 1,
            ),
            next_estate_holding_id: snapshot.next_estate_holding_id.max(
                estate_holdings
                    .iter()
                    .map(|holding| holding.id)
                    .max()
                    .unwrap_or(0)
                    + 1,
            ),
            next_succession_crisis_id: snapshot.next_succession_crisis_id.max(
                succession_crises
                    .iter()
                    .map(|crisis| crisis.id)
                    .max()
                    .unwrap_or(0)
                    + 1,
            ),
            next_power_center_id: snapshot.next_power_center_id.max(
                power_centers
                    .iter()
                    .map(|power| power.id)
                    .max()
                    .unwrap_or(0)
                    + 1,
            ),
            next_authority_office_id: snapshot.next_authority_office_id.max(
                authority_offices
                    .iter()
                    .map(|office| office.id)
                    .max()
                    .unwrap_or(0)
                    + 1,
            ),
            next_item_instance_id: snapshot.next_item_instance_id.max(
                snapshot
                    .item_instances
                    .iter()
                    .map(|item| item.id)
                    .max()
                    .unwrap_or(0)
                    + 1,
            ),
            political_factions,
            political_issues,
            policy_acts,
            territories,
            polities,
            foreign_relations,
            wars,
            military_demands,
            insurrections,
            feudal_titles,
            feudal_contracts,
            estate_holdings,
            succession_crises,
            power_centers,
            authority_offices,
            political_pressures,
            local_norms,
            next_economic_task_id: snapshot.next_economic_task_id,
            next_construction_project_id: snapshot.next_construction_project_id.max(1),
            item_instances: snapshot.item_instances,
            households: snapshot.households,
            establishments: snapshot.establishments,
            village_economy: snapshot.village_economy,
            economic_tasks: snapshot.economic_tasks,
            construction_projects: snapshot.construction_projects,
            pending_thoughts: Vec::new(),
            pending_action_plans: Vec::new(),
            crops: snapshot.crops,
            secrets: snapshot.secrets.clone(),
            caravans: snapshot.caravans.clone(),
            next_secret_id: snapshot.secrets.iter().map(|s| s.id).max().unwrap_or(0) + 1,
            promises: snapshot.promises.clone(),
            policy_favors: snapshot.policy_favors.clone(),
            rumors: snapshot.rumors.clone(),
            cultural_stories: snapshot.cultural_stories.clone(),
            story_versions: snapshot.story_versions.clone(),
            cultural_traditions: snapshot.cultural_traditions.clone(),
            scheduled_meetings,
            active_escrows: snapshot.active_escrows.clone(),
            next_rumor_id: snapshot.rumors.iter().map(|r| r.id).max().unwrap_or(0) + 1,
            next_cultural_story_id: snapshot.next_cultural_story_id.max(
                snapshot
                    .cultural_stories
                    .iter()
                    .map(|story| story.id)
                    .max()
                    .unwrap_or(0)
                    + 1,
            ),
            next_creature_id: snapshot.next_creature_id.max(1),
            next_hunting_quest_id: snapshot.next_hunting_quest_id.max(1),
            hunting_quests: snapshot.hunting_quests.clone(),
        }
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
            let institutional_perception = entry
                .get::<InstitutionalPerceptionComponent>()
                .expect("missing institutional perception component");
            let psychological_state = entry
                .get::<PsychologicalStateComponent>()
                .expect("missing psychological state component");
            let relations = entry
                .get::<RelationComponent>()
                .expect("missing relation component");
            let rumor_beliefs = entry
                .get::<RumorBeliefComponent>()
                .expect("missing rumor belief component");
            let story_beliefs = entry
                .get::<StoryBeliefComponent>()
                .map(|beliefs| beliefs.0.clone())
                .unwrap_or_default();
            let lineage = entry
                .get::<LineageComponent>()
                .expect("missing lineage component");
            let memories = entry
                .get::<MemoryComponent>()
                .expect("missing memory component");
            let inventory = entry
                .get::<InventoryComponent>()
                .expect("missing inventory component");
            let item_inventory = entry
                .get::<ItemInventoryComponent>()
                .expect("missing item inventory component");
            let equipment = entry
                .get::<EquipmentComponent>()
                .expect("missing equipment component");
            let craft_proficiencies = entry
                .get::<CraftProficiencyComponent>()
                .expect("missing craft proficiency component");
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
                institutional_perception: institutional_perception.0.clone(),
                psychological_state: psychological_state.0.clone(),
                rumor_beliefs: rumor_beliefs.0.clone(),
                story_beliefs,
                relations: relations.0.clone(),
                memories: memories.0.clone(),
                inventory: inventory.0.clone(),
                inventory_item_ids: item_inventory.0.clone(),
                equipped_items: equipment.0.clone(),
                craft_proficiencies: craft_proficiencies.0.clone(),
                position: position.0,
                destination: destination.0,
                destination_label: destination_label.0.clone(),
                planned_path: path.0.clone(),
                current_building_id: self.tile_at(position.0).and_then(|tile| tile.building_id),
                current_room_id: self.tile_at(position.0).and_then(|tile| tile.room_id),
                active_conversation_id: conversation.active_conversation_id,
                conversation_participant_ids: conversation.conversation_participant_ids.clone(),
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
                age: lineage.age,
                parents: lineage.parents.clone(),
                children: lineage.children.clone(),
                spouse: lineage.spouse,
                gender: lineage.gender.clone(),
            });
        }

        SimulationSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            catalog_version: self.catalog.version,
            village_name: self.village_name.clone(),
            world_history_years_simulated: self.world_history_years_simulated,
            world_foundation_year: self.world_foundation_year,
            historical_summary: self.historical_summary.clone(),
            day: self.day,
            tick_of_day: self.tick_of_day,
            total_ticks: self.total_ticks,
            ticks_per_day: self.ticks_per_day,
            next_memory_id: self.next_memory_id,
            next_conversation_id: self.next_conversation_id,
            next_economic_task_id: self.next_economic_task_id,
            next_construction_project_id: self.next_construction_project_id,
            next_combat_id: self.next_combat_id,
            next_crime_case_id: self.next_crime_case_id,
            next_political_faction_id: self.next_political_faction_id,
            next_political_issue_id: self.next_political_issue_id,
            next_policy_act_id: self.next_policy_act_id,
            next_territory_id: self.next_territory_id,
            next_polity_id: self.next_polity_id,
            next_foreign_relation_id: self.next_foreign_relation_id,
            next_war_id: self.next_war_id,
            next_military_demand_id: self.next_military_demand_id,
            next_insurrection_id: self.next_insurrection_id,
            next_cultural_story_id: self.next_cultural_story_id,
            next_scheduled_meeting_id: self.next_scheduled_meeting_id,
            next_feudal_title_id: self.next_feudal_title_id,
            next_feudal_contract_id: self.next_feudal_contract_id,
            next_estate_holding_id: self.next_estate_holding_id,
            next_succession_crisis_id: self.next_succession_crisis_id,
            next_power_center_id: self.next_power_center_id,
            next_authority_office_id: self.next_authority_office_id,
            next_item_instance_id: self.next_item_instance_id,
            agents,
            item_instances: self.item_instances.clone(),
            conversations: self.conversations.clone(),
            scheduled_meetings: self.scheduled_meetings.clone(),
            combats: self.combats.clone(),
            crime_cases: self.crime_cases.clone(),
            political_factions: self.political_factions.clone(),
            political_issues: self.political_issues.clone(),
            policy_acts: self.policy_acts.clone(),
            territories: self.territories.clone(),
            polities: self.polities.clone(),
            foreign_relations: self.foreign_relations.clone(),
            wars: self.wars.clone(),
            military_demands: self.military_demands.clone(),
            insurrections: self.insurrections.clone(),
            feudal_titles: self.feudal_titles.clone(),
            feudal_contracts: self.feudal_contracts.clone(),
            estate_holdings: self.estate_holdings.clone(),
            succession_crises: self.succession_crises.clone(),
            power_centers: self.power_centers.clone(),
            authority_offices: self.authority_offices.clone(),
            political_pressures: self.political_pressures.clone(),
            local_norms: self.local_norms.clone(),
            households: self.households.clone(),
            establishments: self.establishments.clone(),
            village_economy: self.village_economy.clone(),
            economic_tasks: self.economic_tasks.clone(),
            construction_projects: self.construction_projects.clone(),
            spatial: self.spatial.clone(),
            events: self.events.clone(),
            crops: self.crops.clone(),
            secrets: self.secrets.clone(),
            caravans: self.caravans.clone(),
            promises: self.promises.clone(),
            policy_favors: self.policy_favors.clone(),
            rumors: self.rumors.clone(),
            cultural_stories: self.cultural_stories.clone(),
            story_versions: self.story_versions.clone(),
            cultural_traditions: self.cultural_traditions.clone(),
            active_escrows: self.active_escrows.clone(),
            next_creature_id: self.next_creature_id,
            creatures: {
                let mut list = Vec::new();
                let mut creature_query = self.world.query::<(
                    &CreatureCore,
                    &CreatureStateComponent,
                    &InjuryComponent,
                    &PositionComponent,
                    &LifeStatusComponent,
                )>();
                for (core, state, injury, position, life) in creature_query.iter(&self.world) {
                    list.push(Creature {
                        id: core.id,
                        name: core.name.clone(),
                        species: core.species.clone(),
                        is_legendary: core.is_legendary,
                        health: state.health,
                        max_health: state.max_health,
                        attack_power: state.attack_power,
                        position: position.0,
                        habitat_territory_id: core.habitat_territory_id,
                        active: life.0 == AgentLifeStatus::Vivo,
                        injury: injury.0.clone(),
                    });
                }
                list
            },
            next_hunting_quest_id: self.next_hunting_quest_id,
            hunting_quests: self.hunting_quests.clone(),
        }
    }
}
