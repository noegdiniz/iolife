use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type BuildingId = u64;
pub type RoomId = u64;
pub type FixtureId = u64;
pub type ConversationId = u64;
pub type EstablishmentId = u64;
pub type EconomicTaskId = u64;
pub type CombatId = u64;
pub type CrimeCaseId = u64;
pub type PoliticalFactionId = u64;
pub type PoliticalIssueId = u64;

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
    pub fn id(self) -> &'static str {
        match self {
            Self::Farmer => "campones",
            Self::Blacksmith => "ferreiro",
            Self::Baker => "padeiro",
            Self::TavernKeeper => "taverneiro",
            Self::Guard => "guarda",
            Self::Headman => "lider_local",
        }
    }

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDef {
    pub id: String,
    pub display_name: String,
    pub tags: Vec<String>,
    pub base_price: i32,
    pub consumption_priority: i32,
    pub can_buy_external: bool,
    pub can_sell_external: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleDef {
    pub id: String,
    pub display_name: String,
    pub allowed_establishment_type_ids: Vec<String>,
    pub can_take_logistics_tasks: bool,
    pub can_collect_payments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeInputDef {
    pub resource_id: String,
    pub amount: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeDef {
    pub id: String,
    pub establishment_type_id: String,
    pub output_resource_id: String,
    pub output_amount: i32,
    pub inputs: Vec<RecipeInputDef>,
    pub capital_requirements: Vec<RecipeInputDef>,
    pub labor_cost: i32,
    pub tool_wear: i32,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OwnerPolicyDef {
    PrivateByRole { role_id: String },
    SharedByRoles { role_ids: Vec<String> },
    Civic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpatialArchetypeDef {
    pub id: String,
    pub display_name: String,
    pub location_kind: LocationKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstablishmentTypeDef {
    pub id: String,
    pub display_name: String,
    pub spatial_archetype_id: String,
    pub location_kind: LocationKind,
    pub public_service: bool,
    pub owner_policy: OwnerPolicyDef,
    pub wage_per_shift: i32,
    pub stock_targets: Vec<ResourceStack>,
    pub default_stock: Vec<ResourceStack>,
    pub production_recipe_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMarketRule {
    pub resource_id: String,
    pub buy_price: i32,
    pub sell_price: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedAgentDef {
    pub id: u64,
    pub name: String,
    pub role_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomyCatalog {
    pub version: u32,
    pub resources: Vec<ResourceDef>,
    pub roles: Vec<RoleDef>,
    pub spatial_archetypes: Vec<SpatialArchetypeDef>,
    pub establishment_types: Vec<EstablishmentTypeDef>,
    pub recipes: Vec<RecipeDef>,
    pub external_market_rules: Vec<ExternalMarketRule>,
    pub seeded_agents: Vec<SeedAgentDef>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CropStage {
    Planted,
    Growing,
    Ready,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CropState {
    pub stage: CropStage,
    pub ticks_since_planted: u32,
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
    pub fn id(self) -> &'static str {
        self.as_str()
    }

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
    pub resource_id: String,
    pub amount: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostedPrice {
    pub resource_id: String,
    pub unit_price: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMarketQuote {
    pub resource_id: String,
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
    pub resource_id: String,
    pub pressure: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HouseholdEconomy {
    pub id: BuildingId,
    pub name: String,
    pub member_ids: Vec<u64>,
    pub treasury: i32,
    pub pantry: Vec<ResourceStack>,
    #[serde(default)]
    pub reserved_food: Vec<ResourceStack>,
    pub minimum_food_units: i32,
    pub pending_payments: Vec<PendingPaymentClaim>,
    pub scarcity_pressure: i32,
    #[serde(default)]
    pub food_crisis_level: u8,
    #[serde(default)]
    pub reserved_food_workers: u8,
    #[serde(default)]
    pub last_food_shortage_tick: u64,
    pub tax_arrears: i32,
    pub last_tax_paid_day: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstablishmentEconomy {
    pub id: EstablishmentId,
    pub building_id: Option<BuildingId>,
    pub name: String,
    pub establishment_type_id: String,
    pub location_kind: LocationKind,
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
pub enum EconomicTaskClass {
    HouseholdFoodPurchase,
    FoodSupplyTransport,
    FoodProduction,
    EssentialProduction,
    GeneralCommerce,
    SurplusSale,
    PaymentCollection,
}

impl EconomicTaskClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HouseholdFoodPurchase => "household_food_purchase",
            Self::FoodSupplyTransport => "food_supply_transport",
            Self::FoodProduction => "food_production",
            Self::EssentialProduction => "essential_production",
            Self::GeneralCommerce => "general_commerce",
            Self::SurplusSale => "surplus_sale",
            Self::PaymentCollection => "payment_collection",
        }
    }

    pub fn is_food_support(self) -> bool {
        matches!(
            self,
            Self::HouseholdFoodPurchase | Self::FoodSupplyTransport
        )
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
    #[serde(default = "default_economic_task_class")]
    pub class: EconomicTaskClass,
    #[serde(default = "default_task_priority")]
    pub priority: u8,
    #[serde(default = "default_lock_until_complete")]
    pub lock_until_complete: bool,
    #[serde(default)]
    pub creates_household_reserve: bool,
    pub actor_household_id: BuildingId,
    pub assigned_agent_id: Option<u64>,
    pub source: EconomicNode,
    pub destination: EconomicNode,
    pub resource_id: Option<String>,
    pub amount: i32,
    pub unit_price: i32,
    pub total_price: i32,
    pub description: String,
    pub phase: EconomicTaskPhase,
    pub related_establishment_id: Option<EstablishmentId>,
}

fn default_economic_task_class() -> EconomicTaskClass {
    EconomicTaskClass::GeneralCommerce
}

fn default_task_priority() -> u8 {
    50
}

fn default_lock_until_complete() -> bool {
    true
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
    #[serde(default)]
    pub trauma_traits: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TraumaTracker {
    pub consecutive_starving_ticks: u32,
    pub consecutive_stressed_ticks: u32,
    pub consecutive_wealthy_ticks: u32,
    pub violence_witnessed_count: u32,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum AgentLifeStatus {
    #[default]
    Vivo,
    Incapacitado,
    Morto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct InjuryState {
    pub light_wounds: u8,
    pub severe_wounds: u8,
    pub pain: i32,
    pub bleeding: i32,
    pub recovery_ticks: u32,
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
    Agredir,
    Combater,
    Roubar,
    Furtar,
    Fugir,
    Acusar,
    Investigar,
    Prender,
    Punir,
    Apoiar,
    Opor,
    Pressionar,
    PedirApoio,
    Mediar,
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
            Self::Agredir => "agredir",
            Self::Combater => "combater",
            Self::Roubar => "roubar",
            Self::Furtar => "furtar",
            Self::Fugir => "fugir",
            Self::Acusar => "acusar",
            Self::Investigar => "investigar",
            Self::Prender => "prender",
            Self::Punir => "punir",
            Self::Apoiar => "apoiar",
            Self::Opor => "opor",
            Self::Pressionar => "pressionar",
            Self::PedirApoio => "pedir_apoio",
            Self::Mediar => "mediar",
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SimplifiedTask {
    pub kind: IntentKind,
    pub target_semantic: Option<String>,
    pub target_agent: Option<u64>,
    pub social_move: Option<SocialMove>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CombatStatus {
    Active,
    Ended,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CombatOutcome {
    Ongoing,
    Fled,
    Incapacitation,
    Death,
    DistanceBreak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatState {
    pub id: CombatId,
    pub participants: [u64; 2],
    pub aggressor_id: u64,
    pub started_at_tick: u64,
    pub round: u32,
    pub status: CombatStatus,
    pub outcome: CombatOutcome,
    pub end_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CrimeType {
    Assault,
    Theft,
    Robbery,
    Homicide,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CrimeCaseStatus {
    Open,
    Investigating,
    Proven,
    Arrested,
    Punished,
    Closed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SentenceKind {
    None,
    Restitution,
    Fine,
    Detention,
    Corporal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrimeCase {
    pub id: CrimeCaseId,
    pub crime_type: CrimeType,
    pub victim_id: Option<u64>,
    pub suspect_id: Option<u64>,
    pub witnesses: Vec<u64>,
    pub evidence: Vec<String>,
    pub severity: u8,
    pub confidence: u8,
    pub status: CrimeCaseStatus,
    pub sentence: SentenceKind,
    pub opened_day: u32,
    pub opened_tick: u32,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PolicyDomain {
    Tax,
    Justice,
    Rationing,
}

impl PolicyDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tax => "imposto",
            Self::Justice => "justica",
            Self::Rationing => "racionamento",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum JusticeSeverity {
    Lenient,
    #[default]
    Normal,
    Severe,
}

impl JusticeSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lenient => "branda",
            Self::Normal => "normal",
            Self::Severe => "severa",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum RationingPolicy {
    HouseholdFirst,
    ProducersFirst,
    CivicFirst,
    #[default]
    Balanced,
}

impl RationingPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HouseholdFirst => "lares",
            Self::ProducersFirst => "produtores",
            Self::CivicFirst => "civico",
            Self::Balanced => "equilibrado",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LocalNorms {
    pub justice_severity: JusticeSeverity,
    pub rationing_policy: RationingPolicy,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PoliticalIssueStatus {
    Open,
    Passed,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoliticalPressure {
    pub actor_id: u64,
    pub household_id: Option<BuildingId>,
    pub agenda_tag: String,
    pub domain: PolicyDomain,
    pub proposed_value: String,
    pub intensity: i32,
    pub reason: String,
    pub day: u32,
    pub tick: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoliticalIssue {
    pub id: PoliticalIssueId,
    pub agenda_tag: String,
    pub domain: PolicyDomain,
    pub proposed_value: String,
    pub summary: String,
    pub proposed_by: Option<u64>,
    pub support_score: i32,
    pub opposition_score: i32,
    pub supporter_ids: Vec<u64>,
    pub opposer_ids: Vec<u64>,
    pub status: PoliticalIssueStatus,
    pub opened_day: u32,
    pub resolved_day: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FactionObjective {
    FoodRiot {
        barn_building_id: BuildingId,
        target_grains: i32,
        grains_stolen: i32,
    },
    TaxBoycott {
        day_activated: u32,
    },
    DeposeLeader {
        leader_agent_id: u64,
    },
    VigilanteJustice {
        suspect_agent_id: u64,
        crime_case_id: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoliticalFaction {
    pub id: PoliticalFactionId,
    pub name: String,
    pub agenda_tag: String,
    pub domain: PolicyDomain,
    pub proposed_value: String,
    pub founder_id: u64,
    pub member_ids: Vec<u64>,
    pub influence: i32,
    pub support_issue_ids: Vec<PoliticalIssueId>,
    pub opposition_issue_ids: Vec<PoliticalIssueId>,
    #[serde(default)]
    pub objective: Option<FactionObjective>,
    #[serde(default)]
    pub is_action_active: bool,
    #[serde(default)]
    pub rage: i32,
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
    Violence,
    Theft,
    CrimeReported,
    Investigation,
    Arrest,
    Punishment,
    Death,
    PoliticalPressure,
    FactionShift,
    PolicyProposal,
    PoliticalSupport,
    NormChanged,
    InstitutionalDispute,
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
    pub role_id: String,
    pub home_building_id: Option<BuildingId>,
    pub work_building_id: Option<BuildingId>,
    pub home_bed: Option<TileCoord>,
    pub profile: AgentProfile,
    pub state: AgentState,
    pub life_status: AgentLifeStatus,
    pub injury: InjuryState,
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
    #[serde(default)]
    pub task_queue: Vec<SimplifiedTask>,
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
    #[serde(default)]
    pub trauma_tracker: TraumaTracker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationSnapshot {
    pub schema_version: u32,
    pub catalog_version: u32,
    pub village_name: String,
    pub day: u32,
    pub tick_of_day: u32,
    pub total_ticks: u64,
    pub ticks_per_day: u32,
    pub next_memory_id: u64,
    pub next_conversation_id: ConversationId,
    pub next_economic_task_id: EconomicTaskId,
    pub next_combat_id: CombatId,
    pub next_crime_case_id: CrimeCaseId,
    pub next_political_faction_id: PoliticalFactionId,
    pub next_political_issue_id: PoliticalIssueId,
    pub agents: Vec<AgentSnapshot>,
    pub conversations: Vec<ConversationState>,
    pub combats: Vec<CombatState>,
    pub crime_cases: Vec<CrimeCase>,
    pub political_factions: Vec<PoliticalFaction>,
    pub political_issues: Vec<PoliticalIssue>,
    pub political_pressures: Vec<PoliticalPressure>,
    pub local_norms: LocalNorms,
    pub households: Vec<HouseholdEconomy>,
    pub establishments: Vec<EstablishmentEconomy>,
    pub village_economy: VillageEconomy,
    pub economic_tasks: Vec<EconomicTask>,
    pub spatial: SpatialSnapshot,
    pub events: Vec<WorldEvent>,
    #[serde(default)]
    pub crops: HashMap<TileCoord, CropState>,
}

fn default_carrying_capacity() -> i32 {
    4
}
