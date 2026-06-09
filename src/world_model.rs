use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type BuildingId = u64;
pub type RoomId = u64;
pub type FixtureId = u64;
pub type ConversationId = u64;
pub type EstablishmentId = u64;
pub type EconomicTaskId = u64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Role {
    Farmer,
    Blacksmith,
    Baker,
    TavernKeeper,
    Guard,
    Headman,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Farmer => "Campones",
            Self::Blacksmith => "Ferreiro",
            Self::Baker => "Padeiro",
            Self::TavernKeeper => "Taverneiro",
            Self::Guard => "Guarda",
            Self::Headman => "Lider Local",
        }
    }

    pub fn all() -> [Self; 6] {
        [
            Self::Farmer,
            Self::Blacksmith,
            Self::Baker,
            Self::TavernKeeper,
            Self::Guard,
            Self::Headman,
        ]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum LocationKind {
    Home,
    Workshop,
    Bakery,
    Tavern,
    Farm,
    Woodlot,
    Quarry,
    Common,
    GuardPost,
    Manor,
}

impl LocationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Home => "Casa",
            Self::Workshop => "Oficina",
            Self::Bakery => "Padaria",
            Self::Tavern => "Taverna",
            Self::Farm => "Campo",
            Self::Woodlot => "Lenhal",
            Self::Quarry => "Pedreira",
            Self::Common => "Praca",
            Self::GuardPost => "Posto da Guarda",
            Self::Manor => "Solar",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct TileCoord {
    pub x: i32,
    pub y: i32,
}

impl TileCoord {
    pub fn manhattan(self, other: TileCoord) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }

    pub fn neighbors4(self) -> [TileCoord; 4] {
        [
            TileCoord {
                x: self.x + 1,
                y: self.y,
            },
            TileCoord {
                x: self.x - 1,
                y: self.y,
            },
            TileCoord {
                x: self.x,
                y: self.y + 1,
            },
            TileCoord {
                x: self.x,
                y: self.y - 1,
            },
        ]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TileKind {
    Grass,
    Road,
    Floor,
    Wall,
    Door,
    Field,
    Forest,
    Rock,
}

impl TileKind {
    pub fn glyph(self) -> char {
        match self {
            Self::Grass => ' ',
            Self::Road => '=',
            Self::Floor => '.',
            Self::Wall => '#',
            Self::Door => '+',
            Self::Field => ',',
            Self::Forest => '^',
            Self::Rock => '%',
        }
    }

    pub fn walkable(self) -> bool {
        matches!(
            self,
            Self::Grass
                | Self::Road
                | Self::Floor
                | Self::Door
                | Self::Field
                | Self::Forest
                | Self::Rock
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileSpec {
    pub coord: TileCoord,
    pub kind: TileKind,
    pub building_id: Option<BuildingId>,
    pub room_id: Option<RoomId>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FixtureKind {
    Bed,
    Table,
    Workstation,
    Storage,
    Seat,
}

impl FixtureKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bed => "cama",
            Self::Table => "mesa",
            Self::Workstation => "estacao de trabalho",
            Self::Storage => "estoque",
            Self::Seat => "assento",
        }
    }

    pub fn glyph(self) -> char {
        match self {
            Self::Bed => 'b',
            Self::Table => 't',
            Self::Workstation => 'w',
            Self::Storage => 's',
            Self::Seat => 'c',
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Graos,
    Lenha,
    MetalBruto,
    Pao,
    Caldo,
    Ferramentas,
    Moedas,
}

impl ResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Graos => "graos",
            Self::Lenha => "lenha",
            Self::MetalBruto => "metal_bruto",
            Self::Pao => "pao",
            Self::Caldo => "caldo",
            Self::Ferramentas => "ferramentas",
            Self::Moedas => "moedas",
        }
    }

    pub fn is_food(self) -> bool {
        matches!(self, Self::Graos | Self::Pao | Self::Caldo)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStack {
    pub kind: ResourceKind,
    pub amount: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostedPrice {
    pub resource: ResourceKind,
    pub unit_price: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMarketQuote {
    pub resource: ResourceKind,
    pub buy_price: i32,
    pub sell_price: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPaymentClaim {
    pub payer_establishment_id: Option<EstablishmentId>,
    pub payer_label: String,
    pub amount: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScarcityMetric {
    pub resource: ResourceKind,
    pub pressure: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HouseholdEconomy {
    pub id: BuildingId,
    pub name: String,
    pub member_ids: Vec<u64>,
    pub treasury: i32,
    pub pantry: Vec<ResourceStack>,
    pub minimum_food_units: i32,
    pub pending_payments: Vec<PendingPaymentClaim>,
    pub scarcity_pressure: i32,
    pub tax_arrears: i32,
    pub last_tax_paid_day: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstablishmentEconomy {
    pub id: EstablishmentId,
    pub building_id: Option<BuildingId>,
    pub name: String,
    pub kind: LocationKind,
    pub owner_household_ids: Vec<BuildingId>,
    pub storage_fixture_id: Option<FixtureId>,
    pub cash: i32,
    pub stock: Vec<ResourceStack>,
    pub stock_targets: Vec<ResourceStack>,
    pub posted_prices: Vec<PostedPrice>,
    pub wage_per_shift: i32,
    pub tool_wear: i32,
    pub public_service: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VillageEconomy {
    pub public_treasury: i32,
    pub daily_household_tax: i32,
    pub external_market_coord: TileCoord,
    pub base_prices: Vec<PostedPrice>,
    pub external_quotes: Vec<ExternalMarketQuote>,
    pub scarcity_metrics: Vec<ScarcityMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EconomicNode {
    HouseholdPantry(BuildingId),
    Establishment(EstablishmentId),
    ExternalMarket,
    PublicTreasury,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EconomicTaskKind {
    Produzir,
    Comprar,
    Transportar,
    Vender,
    ReceberPagamento,
}

impl EconomicTaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Produzir => "produzir",
            Self::Comprar => "comprar",
            Self::Transportar => "transportar",
            Self::Vender => "vender",
            Self::ReceberPagamento => "receber_pagamento",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EconomicTaskPhase {
    AwaitingPickup,
    InTransit,
    AwaitingPayment,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicTask {
    pub id: EconomicTaskId,
    pub kind: EconomicTaskKind,
    pub actor_household_id: BuildingId,
    pub assigned_agent_id: Option<u64>,
    pub source: EconomicNode,
    pub destination: EconomicNode,
    pub resource: Option<ResourceKind>,
    pub amount: i32,
    pub unit_price: i32,
    pub total_price: i32,
    pub description: String,
    pub phase: EconomicTaskPhase,
    pub related_establishment_id: Option<EstablishmentId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureSpec {
    pub id: FixtureId,
    pub building_id: Option<BuildingId>,
    pub room_id: Option<RoomId>,
    pub kind: FixtureKind,
    pub coord: TileCoord,
    pub name: String,
    pub blocks_movement: bool,
    pub stock: Vec<ResourceStack>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSpec {
    pub id: RoomId,
    pub building_id: BuildingId,
    pub name: String,
    pub kind: String,
    pub tiles: Vec<TileCoord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildingSpec {
    pub id: BuildingId,
    pub name: String,
    pub kind: LocationKind,
    pub entrance: TileCoord,
    pub room_ids: Vec<RoomId>,
    pub footprint: Vec<TileCoord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldGrid {
    pub width: i32,
    pub height: i32,
    pub tiles: Vec<TileSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpatialSnapshot {
    pub grid: WorldGrid,
    pub buildings: Vec<BuildingSpec>,
    pub rooms: Vec<RoomSpec>,
    pub fixtures: Vec<FixtureSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub traits: Vec<String>,
    pub values: Vec<String>,
    pub fears: Vec<String>,
    pub long_term_desires: Vec<String>,
    pub moral_tolerances: Vec<String>,
    pub social_style: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub mood: i32,
    pub energy: i32,
    pub health: i32,
    pub hunger: i32,
    pub stress: i32,
    pub current_focus: String,
    pub active_goals: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MemoryKind {
    Fact,
    Impression,
    Promise,
    Offense,
    Success,
    Failure,
    Reflection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemory {
    pub id: u64,
    pub day: u32,
    pub tick: u32,
    pub kind: MemoryKind,
    pub summary: String,
    pub details: String,
    pub emotional_weight: i32,
    pub about: Vec<u64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentRelation {
    pub trust: i32,
    pub friendship: i32,
    pub resentment: i32,
    pub attraction: i32,
    pub moral_debt: i32,
    pub reputation: i32,
    pub last_updated_day: u32,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IntentKind {
    Trabalhar,
    Descansar,
    Comer,
    Socializar,
    Refletir,
    Andar,
    Comprar,
    Transportar,
    Vender,
    ReceberPagamento,
}

impl IntentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trabalhar => "trabalhar",
            Self::Descansar => "descansar",
            Self::Comer => "comer",
            Self::Socializar => "socializar",
            Self::Refletir => "refletir",
            Self::Andar => "andar",
            Self::Comprar => "comprar",
            Self::Transportar => "transportar",
            Self::Vender => "vender",
            Self::ReceberPagamento => "receber_pagamento",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SocialMove {
    Chat,
    Gossip,
    Promise,
    Offend,
    Reconcile,
    Favor,
}

impl SocialMove {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "conversar",
            Self::Gossip => "fofocar",
            Self::Promise => "prometer",
            Self::Offend => "ofender",
            Self::Reconcile => "reconciliar",
            Self::Favor => "ajudar",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIntent {
    pub kind: IntentKind,
    pub target_agent: Option<u64>,
    pub target_semantic: Option<String>,
    pub justification: String,
    pub dominant_emotion: String,
    pub perceived_risk: u8,
    pub belief_updates: Vec<String>,
    pub priority: u8,
    pub social_move: Option<SocialMove>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RelationDelta {
    pub trust: i32,
    pub friendship: i32,
    pub resentment: i32,
    pub attraction: i32,
    pub moral_debt: i32,
    pub reputation: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConversationStatus {
    Active,
    Ended,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConversationOutcome {
    Ongoing,
    MutualEnd,
    OneSidedExit,
    MaxTurns,
    DistanceBreak,
    BlockingBreak,
    CriticalNeed,
    PhysicalConflict,
    ProviderTimeout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub speaker_id: u64,
    pub listener_id: u64,
    pub tick: u64,
    pub utterance: String,
    pub speech_act: String,
    pub emotion: String,
    pub tone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationParticipantState {
    pub agent_id: u64,
    pub social_goal: String,
    pub last_speech_act: Option<String>,
    pub last_emotion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationState {
    pub id: ConversationId,
    pub participants: [u64; 2],
    pub initiator_id: u64,
    pub current_speaker_id: u64,
    pub started_at_tick: u64,
    pub turn_count: u32,
    pub max_turns: u32,
    pub opening_reason: String,
    pub summary: String,
    pub recent_turns: Vec<ConversationTurn>,
    pub participant_states: Vec<ConversationParticipantState>,
    pub status: ConversationStatus,
    pub outcome: ConversationOutcome,
    pub end_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventKind {
    Routine,
    SocialBond,
    Conflict,
    Commerce,
    Tax,
    Salary,
    Logistics,
    Scarcity,
    PriceUpdate,
    Reflection,
    Need,
    Travel,
    Arrival,
    Blocking,
    CognitionFailure,
    ConversationStarted,
    ConversationTurn,
    ConversationEnded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEvent {
    pub day: u32,
    pub tick: u32,
    pub actor: u64,
    pub target: Option<u64>,
    pub kind: EventKind,
    pub summary: String,
    pub impact_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub id: u64,
    pub name: String,
    pub role: Role,
    pub home_building_id: Option<BuildingId>,
    pub work_building_id: Option<BuildingId>,
    pub home_bed: Option<TileCoord>,
    pub profile: AgentProfile,
    pub state: AgentState,
    pub relations: HashMap<u64, AgentRelation>,
    pub memories: Vec<AgentMemory>,
    pub inventory: Vec<ResourceStack>,
    pub position: TileCoord,
    pub destination: Option<TileCoord>,
    pub destination_label: Option<String>,
    pub planned_path: Vec<TileCoord>,
    pub current_building_id: Option<BuildingId>,
    pub current_room_id: Option<RoomId>,
    pub active_conversation_id: Option<ConversationId>,
    pub conversation_partner_id: Option<u64>,
    pub last_social_act: Option<String>,
    pub social_cooldown_until: u64,
    pub last_intent: Option<AgentIntent>,
    pub last_thought: String,
    pub llm_cooldown_until: u64,
    pub llm_calls: u64,
    #[serde(default)]
    pub active_economic_task_id: Option<EconomicTaskId>,
    #[serde(default)]
    pub carrying: Vec<ResourceStack>,
    #[serde(default = "default_carrying_capacity")]
    pub carrying_capacity: i32,
    #[serde(default)]
    pub next_reconsideration_tick: u64,
    #[serde(default)]
    pub blocked_ticks: u32,
    #[serde(default)]
    pub last_cognition_trigger: Option<String>,
    #[serde(default)]
    pub last_social_opportunity_signature: Option<String>,
    #[serde(default)]
    pub last_deliberation_hunger: i32,
    #[serde(default)]
    pub last_deliberation_energy: i32,
    #[serde(default)]
    pub last_deliberation_health: i32,
    #[serde(default)]
    pub last_deliberation_stress: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationSnapshot {
    pub schema_version: u32,
    pub village_name: String,
    pub day: u32,
    pub tick_of_day: u32,
    pub total_ticks: u64,
    pub ticks_per_day: u32,
    pub next_memory_id: u64,
    pub next_conversation_id: ConversationId,
    pub next_economic_task_id: EconomicTaskId,
    pub agents: Vec<AgentSnapshot>,
    pub conversations: Vec<ConversationState>,
    pub households: Vec<HouseholdEconomy>,
    pub establishments: Vec<EstablishmentEconomy>,
    pub village_economy: VillageEconomy,
    pub economic_tasks: Vec<EconomicTask>,
    pub spatial: SpatialSnapshot,
    pub events: Vec<WorldEvent>,
}

fn default_carrying_capacity() -> i32 {
    4
}
