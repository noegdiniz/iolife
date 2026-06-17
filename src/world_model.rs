use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ===== Identifiers =====

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
pub type PolicyActId = u64;
pub type TerritoryId = u64;
pub type PolityId = u64;
pub type ForeignRelationId = u64;
pub type WarId = u64;
pub type MilitaryDemandId = u64;
pub type InsurrectionId = u64;
pub type ScheduledMeetingId = u64;
pub type FeudalTitleId = u64;
pub type FeudalContractId = u64;
pub type EstateHoldingId = u64;
pub type SuccessionCrisisId = u64;
pub type PowerCenterId = u64;
pub type AuthorityOfficeId = u64;

// ===== Semantic legacy enums =====
//
// The data-driven economy uses catalog string IDs as the source of truth. These
// enums remain for compatibility with spatial semantics and seed defaults.

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

// ===== Economy catalog =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDef {
    pub id: String,
    pub display_name: String,
    pub tags: Vec<String>,
    #[serde(default)]
    pub affordances: Vec<ItemAffordanceDef>,
    pub base_price: i32,
    pub consumption_priority: i32,
    pub can_buy_external: bool,
    pub can_sell_external: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ItemAffordanceKind {
    Food,
    Fuel,
    Tool,
    ConstructionMaterial,
    ImprovisedWeapon,
    Currency,
    TradeGood,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemAffordanceDef {
    pub kind: ItemAffordanceKind,
    pub strength: i32,
    pub consumes_on_use: bool,
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
pub struct ConstructionRecipeDef {
    pub id: String,
    pub establishment_type_id: String,
    pub materials: Vec<RecipeInputDef>,
    pub labor_cost: i32,
    pub required_fixtures: Vec<FixtureKind>,
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
    #[serde(default)]
    pub production_recipe_ids: Vec<String>,
    #[serde(default)]
    pub construction_recipe_id: Option<String>,
    #[serde(default)]
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
    #[serde(default)]
    pub construction_recipes: Vec<ConstructionRecipeDef>,
    pub external_market_rules: Vec<ExternalMarketRule>,
    pub seeded_agents: Vec<SeedAgentDef>,
}

// ===== Spatial model =====

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
    Madeira,
    Pedra,
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
            Self::Madeira => "madeira",
            Self::Pedra => "pedra",
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

// ===== Economy runtime state =====

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
    #[serde(default)]
    pub direct_lord_agent_id: Option<u64>,
    #[serde(default)]
    pub feudal_tribute_due: i32,
    #[serde(default)]
    pub corvee_days_due: i32,
    #[serde(default)]
    pub levy_service_due: i32,
    #[serde(default)]
    pub feudal_arrears: i32,
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
    ConstructionProject(u64),
    MilitarySupply(WarId),
    ExternalMarket,
    PublicTreasury,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EconomicTaskKind {
    Produzir,
    Comprar,
    Transportar,
    Construir,
    Vender,
    ReceberPagamento,
}

impl EconomicTaskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Produzir => "produzir",
            Self::Comprar => "comprar",
            Self::Transportar => "transportar",
            Self::Construir => "construir",
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
    Construction,
    MilitarySupply,
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
            Self::Construction => "construction",
            Self::MilitarySupply => "military_supply",
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
    #[serde(default)]
    pub related_construction_project_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConstructionStatus {
    Planned,
    GatheringMaterials,
    UnderConstruction,
    Completed,
    Blocked,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstructionProject {
    pub id: u64,
    pub establishment_type_id: String,
    pub building_name: String,
    pub planned_footprint: Vec<TileCoord>,
    pub entrance: TileCoord,
    pub materials_required: Vec<ResourceStack>,
    pub materials_delivered: Vec<ResourceStack>,
    pub labor_required: i32,
    pub labor_done: i32,
    pub status: ConstructionStatus,
    pub priority: u8,
    pub systemic_reason: String,
    pub resulting_building_id: Option<BuildingId>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum WorldPlaceKind {
    Building,
    Room,
    Fixture,
    Territory,
    Special,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldPlaceRef {
    pub place_id: String,
    pub display_name: String,
    pub kind: WorldPlaceKind,
    pub semantic_tags: Vec<String>,
    pub building_id: Option<BuildingId>,
    pub room_id: Option<RoomId>,
    pub fixture_id: Option<FixtureId>,
    pub territory_id: Option<TerritoryId>,
}

// ===== Agent state and memory =====

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PsychologicalState {
    pub grief: i32,
    pub humiliation: i32,
    pub fear: i32,
    pub pride: i32,
    pub trauma: i32,
    pub anger: i32,
    pub hope: i32,
    pub guilt: i32,
    pub last_updated_day: u32,
    pub notes: Vec<String>,
}

impl PsychologicalState {
    pub fn zero_delta() -> Self {
        Self::default()
    }

    pub fn clamp_all(&mut self) {
        self.grief = self.grief.clamp(0, 100);
        self.humiliation = self.humiliation.clamp(0, 100);
        self.fear = self.fear.clamp(0, 100);
        self.pride = self.pride.clamp(0, 100);
        self.trauma = self.trauma.clamp(0, 100);
        self.anger = self.anger.clamp(0, 100);
        self.hope = self.hope.clamp(0, 100);
        self.guilt = self.guilt.clamp(0, 100);
        if self.notes.len() > 20 {
            let keep_from = self.notes.len() - 20;
            self.notes.drain(0..keep_from);
        }
    }

    pub fn add_delta(&mut self, delta: &PsychologicalState, day: u32, note: String) {
        self.grief += delta.grief;
        self.humiliation += delta.humiliation;
        self.fear += delta.fear;
        self.pride += delta.pride;
        self.trauma += delta.trauma;
        self.anger += delta.anger;
        self.hope += delta.hope;
        self.guilt += delta.guilt;
        self.last_updated_day = day;
        if !note.is_empty() {
            self.notes.push(note);
        }
        self.clamp_all();
    }

    pub fn decay_daily(&mut self) {
        self.grief = (self.grief - 2).max(0);
        self.humiliation = (self.humiliation - 2).max(0);
        self.fear = (self.fear - 1).max(0);
        self.pride = (self.pride - 1).max(0);
        self.anger = (self.anger - 2).max(0);
        self.hope = (self.hope - 1).max(0);
        self.guilt = (self.guilt - 1).max(0);
        self.trauma = (self.trauma - 1).max(0);
        self.clamp_all();
    }

    pub fn summary(&self) -> String {
        format!(
            "luto={} humilhacao={} medo={} orgulho={} trauma={} raiva={} esperanca={} culpa={}",
            self.grief,
            self.humiliation,
            self.fear,
            self.pride,
            self.trauma,
            self.anger,
            self.hope,
            self.guilt
        )
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstitutionalPerception {
    pub leader_legitimacy: i32,
    pub justice_legitimacy: i32,
    pub tax_legitimacy: i32,
    pub rationing_legitimacy: i32,
    pub guard_trust: i32,
    pub war_support: i32,
    pub fear_of_authority: i32,
    pub perceived_corruption: i32,
    pub perceived_fairness: i32,
    pub last_updated_day: u32,
    pub notes: Vec<String>,
}

impl Default for InstitutionalPerception {
    fn default() -> Self {
        Self {
            leader_legitimacy: 10,
            justice_legitimacy: 10,
            tax_legitimacy: 0,
            rationing_legitimacy: 0,
            guard_trust: 5,
            war_support: 0,
            fear_of_authority: 5,
            perceived_corruption: 0,
            perceived_fairness: 0,
            last_updated_day: 0,
            notes: Vec::new(),
        }
    }
}

impl InstitutionalPerception {
    pub fn zero_delta() -> Self {
        Self {
            leader_legitimacy: 0,
            justice_legitimacy: 0,
            tax_legitimacy: 0,
            rationing_legitimacy: 0,
            guard_trust: 0,
            war_support: 0,
            fear_of_authority: 0,
            perceived_corruption: 0,
            perceived_fairness: 0,
            last_updated_day: 0,
            notes: Vec::new(),
        }
    }

    pub fn clamp_all(&mut self) {
        self.leader_legitimacy = self.leader_legitimacy.clamp(-100, 100);
        self.justice_legitimacy = self.justice_legitimacy.clamp(-100, 100);
        self.tax_legitimacy = self.tax_legitimacy.clamp(-100, 100);
        self.rationing_legitimacy = self.rationing_legitimacy.clamp(-100, 100);
        self.guard_trust = self.guard_trust.clamp(-100, 100);
        self.war_support = self.war_support.clamp(-100, 100);
        self.fear_of_authority = self.fear_of_authority.clamp(-100, 100);
        self.perceived_corruption = self.perceived_corruption.clamp(-100, 100);
        self.perceived_fairness = self.perceived_fairness.clamp(-100, 100);
        if self.notes.len() > 8 {
            let keep_from = self.notes.len() - 8;
            self.notes.drain(0..keep_from);
        }
    }
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
    Construir,
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
    Decretar,
    JurarLealdade,
    RomperLealdade,
    ConcederTitulo,
    RevogarTitulo,
    NomearOficial,
    ExigirTributo,
    CobrarCorveia,
    ConvocarLevy,
    ReconhecerHerdeiro,
    ApoiarPretendente,
    Usurpar,
    ReivindicarTerritorio,
    NegociarSuserania,
    Esconder,
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
            Self::Construir => "construir",
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
            Self::Decretar => "decretar",
            Self::JurarLealdade => "jurar_lealdade",
            Self::RomperLealdade => "romper_lealdade",
            Self::ConcederTitulo => "conceder_titulo",
            Self::RevogarTitulo => "revogar_titulo",
            Self::NomearOficial => "nomear_oficial",
            Self::ExigirTributo => "exigir_tributo",
            Self::CobrarCorveia => "cobrar_corveia",
            Self::ConvocarLevy => "convocar_levy",
            Self::ReconhecerHerdeiro => "reconhecer_herdeiro",
            Self::ApoiarPretendente => "apoiar_pretendente",
            Self::Usurpar => "usurpar",
            Self::ReivindicarTerritorio => "reivindicar_territorio",
            Self::NegociarSuserania => "negociar_suserania",
            Self::Esconder => "esconder",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SocialMove {
    Chat,
    Gossip,
    TellStory,
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
            Self::TellStory => "contar_historia",
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

// ===== Social conversation model =====

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

// ===== Conflict, crime and justice =====

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

// ===== Politics and local norms =====

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
pub enum PolicyActStatus {
    Active,
    Expired,
    Revoked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyAuthority {
    LocalLeader,
    LocalLord,
    RegionalSuzerain,
    Regent,
    MilitaryOccupier,
    InstitutionalVote,
    ForeignPolity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyScope {
    GlobalVillage,
    Holding(EstateHoldingId),
    SeigneurialDomain(TerritoryId),
    Territory(TerritoryId),
    VassalChain(u64),
    Polity(PolityId),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyTarget {
    None,
    Agent(u64),
    Resource(String),
    EstablishmentType(String),
    Territory(TerritoryId),
    Polity(PolityId),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PolicyEffect {
    TaxModifier {
        multiplier_percent: i32,
    },
    RationingRule {
        policy: RationingPolicy,
        energy_gain_percent: i32,
    },
    LaborDraft {
        output_resource_id: String,
        production_bonus_percent: i32,
    },
    MovementRestriction {
        establishment_type_id: String,
    },
    ResourceConfiscation {
        resource_id: String,
        excluded_establishment_type_ids: Vec<String>,
        destination_establishment_type_id: String,
    },
    Mobilization {
        readiness_delta: i32,
    },
    TradeEmbargo {
        polity_id: Option<PolityId>,
        resource_id: Option<String>,
    },
    TerritorialClaim {
        territory_id: TerritoryId,
        claimant_polity_id: PolityId,
    },
    WarDeclaration {
        target_polity_id: PolityId,
    },
    TributeRate {
        amount: i32,
    },
    LaborObligation {
        days: i32,
    },
    LevyCall {
        service_units: i32,
    },
    OfficeAppointment {
        office_id: AuthorityOfficeId,
        holder_agent_id: u64,
    },
    OfficeRevocation {
        office_id: AuthorityOfficeId,
    },
    TitleGrant {
        title_id: FeudalTitleId,
        holder_agent_id: u64,
    },
    TitleRevocation {
        title_id: FeudalTitleId,
    },
    ConfiscationOfHolding {
        holding_id: EstateHoldingId,
    },
    SuccessionRecognition {
        title_id: FeudalTitleId,
        heir_agent_id: u64,
    },
    SuccessionDenial {
        title_id: FeudalTitleId,
        claimant_agent_id: u64,
    },
    FeudalProtectionOrder {
        territory_id: TerritoryId,
        protected_household_id: Option<BuildingId>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyAct {
    pub id: PolicyActId,
    pub agenda_tag: String,
    pub summary: String,
    pub issuer_agent_id: Option<u64>,
    pub issuer_polity_id: Option<PolityId>,
    pub authority: PolicyAuthority,
    pub scope: PolicyScope,
    pub target: PolicyTarget,
    pub effects: Vec<PolicyEffect>,
    pub legitimacy: i32,
    pub enforcement: i32,
    pub resistance: i32,
    pub status: PolicyActStatus,
    pub issued_day: u32,
    pub issued_tick: u32,
    pub expires_day: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerritoryControlPressure {
    pub polity_id: PolityId,
    pub pressure: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Territory {
    pub id: TerritoryId,
    pub name: String,
    pub controller_polity_id: PolityId,
    pub claimed_by: Vec<PolityId>,
    pub building_ids: Vec<BuildingId>,
    pub tile_coords: Vec<TileCoord>,
    pub stability: i32,
    pub strategic_value: i32,
    pub control_pressure: Vec<TerritoryControlPressure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Polity {
    pub id: PolityId,
    pub name: String,
    pub ruler_agent_id: Option<u64>,
    pub capital_territory_id: Option<TerritoryId>,
    pub treasury: i32,
    pub military_readiness: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum FeudalRank {
    Rei,
    Duque,
    Conde,
    Barao,
    Senhor,
    Cavaleiro,
    Oficial,
}

impl FeudalRank {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rei => "rei",
            Self::Duque => "duque",
            Self::Conde => "conde",
            Self::Barao => "barao",
            Self::Senhor => "senhor",
            Self::Cavaleiro => "cavaleiro",
            Self::Oficial => "oficial",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SuccessionRule {
    HerdeiroDireto,
    ConjugeRegente,
    NomeacaoDoSuserano,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FeudalContractStatus {
    Active,
    Breached,
    Revoked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EstateHolderKind {
    Agent,
    Household,
    Polity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AuthorityOfficeKind {
    Intendente,
    Coletor,
    JuizLocal,
    CapitaoDaGuarda,
    Carcereiro,
    AdministradorDoSolar,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SuccessionCrisisStatus {
    Open,
    Resolved,
    Suppressed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeudalTitle {
    pub id: FeudalTitleId,
    pub name: String,
    pub rank: FeudalRank,
    pub holder_agent_id: Option<u64>,
    pub polity_id: Option<PolityId>,
    pub territory_id: Option<TerritoryId>,
    pub holding_id: Option<EstateHoldingId>,
    pub suzerain_title_id: Option<FeudalTitleId>,
    pub succession_rule: SuccessionRule,
    pub legitimacy: i32,
    pub precedence: i32,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeudalContract {
    pub id: FeudalContractId,
    pub suzerain_agent_id: u64,
    pub vassal_agent_id: u64,
    pub territory_id: Option<TerritoryId>,
    pub holding_id: Option<EstateHoldingId>,
    pub tribute_due_per_day: i32,
    pub levy_duty: i32,
    pub judicial_aid_duty: i32,
    pub maintenance_duty: i32,
    pub loyalty: i32,
    pub coercion: i32,
    pub perceived_legitimacy: i32,
    pub status: FeudalContractStatus,
    pub last_updated_day: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstateHolding {
    pub id: EstateHoldingId,
    pub name: String,
    pub holder_kind: EstateHolderKind,
    pub holder_agent_id: Option<u64>,
    pub holder_household_id: Option<BuildingId>,
    pub holder_polity_id: Option<PolityId>,
    pub territory_id: TerritoryId,
    pub building_ids: Vec<BuildingId>,
    pub establishment_ids: Vec<EstablishmentId>,
    pub annualized_value: i32,
    pub tribute_share_percent: i32,
    pub labor_obligation_days: i32,
    pub military_obligation: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessionCrisis {
    pub id: SuccessionCrisisId,
    pub title_id: FeudalTitleId,
    pub territory_id: Option<TerritoryId>,
    pub claimant_ids: Vec<u64>,
    pub recognized_heir_id: Option<u64>,
    pub usurper_id: Option<u64>,
    pub status: SuccessionCrisisStatus,
    pub legitimacy_gap: i32,
    pub conflict_score: i32,
    pub opened_day: u32,
    pub resolved_day: Option<u32>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerCenter {
    pub id: PowerCenterId,
    pub territory_id: Option<TerritoryId>,
    pub title_id: Option<FeudalTitleId>,
    pub agent_id: Option<u64>,
    pub formal_authority: i32,
    pub material_power: i32,
    pub coercive_power: i32,
    pub legitimacy: i32,
    pub stability: i32,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityOffice {
    pub id: AuthorityOfficeId,
    pub kind: AuthorityOfficeKind,
    pub name: String,
    pub granter_agent_id: Option<u64>,
    pub holder_agent_id: Option<u64>,
    pub territory_id: Option<TerritoryId>,
    pub title_id: Option<FeudalTitleId>,
    pub active: bool,
    pub authority_score: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ForeignStance {
    Neutral,
    TradePartner,
    Ally,
    Rival,
    AtWar,
    Tributary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignRelation {
    pub id: ForeignRelationId,
    pub polity_a: PolityId,
    pub polity_b: PolityId,
    pub stance: ForeignStance,
    pub trust: i32,
    pub fear: i32,
    pub grievances: Vec<String>,
    pub treaty_policy_act_ids: Vec<PolicyActId>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum WarStage {
    Mobilization,
    Raids,
    Siege,
    DecisiveBattle,
    Occupation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum WarStatus {
    Active,
    Won,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarState {
    pub id: WarId,
    pub attacker_polity_id: PolityId,
    pub defender_polity_id: PolityId,
    pub target_territory_ids: Vec<TerritoryId>,
    pub attacker_score: i32,
    pub defender_score: i32,
    pub stage: WarStage,
    pub status: WarStatus,
    pub winner_polity_id: Option<PolityId>,
    pub started_day: u32,
    pub ended_day: Option<u32>,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MilitaryDemandStatus {
    Open,
    PartiallySupplied,
    Satisfied,
    Failed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilitaryDemand {
    pub id: MilitaryDemandId,
    pub war_id: WarId,
    pub polity_id: PolityId,
    pub stage: WarStage,
    pub required: Vec<ResourceStack>,
    pub delivered: Vec<ResourceStack>,
    pub cash_required: i32,
    pub cash_delivered: i32,
    pub target_territory_id: Option<TerritoryId>,
    pub priority: u8,
    pub deadline_day: u32,
    pub status: MilitaryDemandStatus,
    pub shortage_score: i32,
    pub created_day: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InsurrectionStage {
    Agitation,
    Riot,
    OrganizedRevolt,
    CivilWar,
    Suppressed,
    Victorious,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InsurrectionStatus {
    Active,
    Suppressed,
    Victorious,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsurrectionState {
    pub id: InsurrectionId,
    pub faction_ids: Vec<PoliticalFactionId>,
    pub target_polity_id: PolityId,
    pub rebel_polity_id: Option<PolityId>,
    pub target_territory_id: TerritoryId,
    pub popular_support: i32,
    pub repression: i32,
    pub stage: InsurrectionStage,
    pub status: InsurrectionStatus,
    pub linked_war_id: Option<WarId>,
    pub started_day: u32,
    pub ended_day: Option<u32>,
    pub summary: String,
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
pub enum ScheduledMeetingStatus {
    Proposed,
    Accepted,
    Rejected,
    Active,
    Completed,
    Missed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledMeeting {
    pub id: ScheduledMeetingId,
    pub proposer_id: u64,
    pub invitee_id: u64,
    pub place_id: String,
    pub scheduled_day: u32,
    pub scheduled_tick: u32,
    pub purpose: String,
    pub status: ScheduledMeetingStatus,
    pub created_tick: u64,
    pub response_tick: Option<u64>,
}

// ===== Events and snapshots =====

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
    Construction,
    MilitarySupply,
    CulturalStory,
    Meeting,
    VassalOath,
    TributeDemanded,
    TributePaid,
    TributeRefused,
    LevyCalled,
    LevyRefused,
    TitleGranted,
    TitleRevoked,
    SuccessionOpened,
    SuccessionRecognized,
    SuccessionContested,
    Usurpation,
    FeudalSanction,
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
    pub institutional_perception: InstitutionalPerception,
    #[serde(default)]
    pub psychological_state: PsychologicalState,
    #[serde(default)]
    pub rumor_beliefs: Vec<RumorBelief>,
    #[serde(default)]
    pub story_beliefs: Vec<StoryBelief>,
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
    #[serde(default = "default_age")]
    pub age: u32,
    #[serde(default)]
    pub parents: Vec<u64>,
    #[serde(default)]
    pub children: Vec<u64>,
    #[serde(default)]
    pub spouse: Option<u64>,
    #[serde(default = "default_gender")]
    pub gender: String,
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
    #[serde(default)]
    pub next_construction_project_id: u64,
    pub next_combat_id: CombatId,
    pub next_crime_case_id: CrimeCaseId,
    pub next_political_faction_id: PoliticalFactionId,
    pub next_political_issue_id: PoliticalIssueId,
    pub next_policy_act_id: PolicyActId,
    pub next_territory_id: TerritoryId,
    pub next_polity_id: PolityId,
    pub next_foreign_relation_id: ForeignRelationId,
    pub next_war_id: WarId,
    pub next_military_demand_id: MilitaryDemandId,
    pub next_insurrection_id: InsurrectionId,
    pub next_cultural_story_id: CulturalStoryId,
    pub next_scheduled_meeting_id: ScheduledMeetingId,
    #[serde(default)]
    pub next_feudal_title_id: FeudalTitleId,
    #[serde(default)]
    pub next_feudal_contract_id: FeudalContractId,
    #[serde(default)]
    pub next_estate_holding_id: EstateHoldingId,
    #[serde(default)]
    pub next_succession_crisis_id: SuccessionCrisisId,
    #[serde(default)]
    pub next_power_center_id: PowerCenterId,
    #[serde(default)]
    pub next_authority_office_id: AuthorityOfficeId,
    pub agents: Vec<AgentSnapshot>,
    pub conversations: Vec<ConversationState>,
    pub scheduled_meetings: Vec<ScheduledMeeting>,
    pub combats: Vec<CombatState>,
    pub crime_cases: Vec<CrimeCase>,
    pub political_factions: Vec<PoliticalFaction>,
    pub political_issues: Vec<PoliticalIssue>,
    pub policy_acts: Vec<PolicyAct>,
    pub territories: Vec<Territory>,
    pub polities: Vec<Polity>,
    pub foreign_relations: Vec<ForeignRelation>,
    pub wars: Vec<WarState>,
    pub military_demands: Vec<MilitaryDemand>,
    pub insurrections: Vec<InsurrectionState>,
    #[serde(default)]
    pub feudal_titles: Vec<FeudalTitle>,
    #[serde(default)]
    pub feudal_contracts: Vec<FeudalContract>,
    #[serde(default)]
    pub estate_holdings: Vec<EstateHolding>,
    #[serde(default)]
    pub succession_crises: Vec<SuccessionCrisis>,
    #[serde(default)]
    pub power_centers: Vec<PowerCenter>,
    #[serde(default)]
    pub authority_offices: Vec<AuthorityOffice>,
    pub political_pressures: Vec<PoliticalPressure>,
    pub local_norms: LocalNorms,
    pub households: Vec<HouseholdEconomy>,
    pub establishments: Vec<EstablishmentEconomy>,
    pub village_economy: VillageEconomy,
    pub economic_tasks: Vec<EconomicTask>,
    #[serde(default)]
    pub construction_projects: Vec<ConstructionProject>,
    pub spatial: SpatialSnapshot,
    pub events: Vec<WorldEvent>,
    #[serde(default)]
    pub crops: HashMap<TileCoord, CropState>,
    #[serde(default)]
    pub secrets: Vec<Secret>,
    #[serde(default)]
    pub caravans: Vec<CaravanState>,
    #[serde(default)]
    pub promises: Vec<ActivePromise>,
    #[serde(default)]
    pub policy_favors: Vec<PolicyFavor>,
    #[serde(default)]
    pub rumors: Vec<Rumor>,
    #[serde(default)]
    pub cultural_stories: Vec<CulturalStory>,
    #[serde(default)]
    pub story_versions: Vec<StoryVersion>,
    #[serde(default)]
    pub cultural_traditions: Vec<CulturalTradition>,
    #[serde(default)]
    pub active_escrows: Vec<EscrowAccount>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SecretKind {
    CrimeCulprit,
    CaravanRoute,
    CorruptionEmbezzle,
    BrokenPromise,
    SlanderCalumny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    pub id: u64,
    pub kind: SecretKind,
    pub target_id: u64,
    pub summary: String,
    pub details: String,
    pub known_by: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaravanState {
    pub id: u64,
    pub resource_id: String,
    pub amount: i32,
    pub escort_ids: Vec<u64>,
    pub position: TileCoord,
    pub destination: TileCoord,
    pub status: String, // "trânsito", "entregue", "saqueada"
}

fn default_carrying_capacity() -> i32 {
    4
}

fn default_age() -> u32 {
    25
}

fn default_gender() -> String {
    "Masculino".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PromiseCondition {
    DeliverResource { resource_id: String, amount: i32 },
    VoteForPolicy { domain: String, value: String },
    KeepSecret { secret_id: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivePromise {
    pub id: u64,
    pub promiser_id: u64,
    pub promisee_id: u64,
    pub condition: PromiseCondition,
    pub deadline_tick: u32,
    pub created_at_tick: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyFavor {
    pub leader_id: u64,
    pub beneficiary_id: u64,
    pub resource_id: String,
    pub priority_score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rumor {
    pub id: u64,
    pub source_agent_id: u64,
    pub current_carrier_ids: Vec<u64>,
    pub claim: String,
    pub topic: String,
    pub target_agent_id: u64,
    pub about_agent_id: Option<u64>,
    pub about_household_id: Option<BuildingId>,
    pub about_policy_act_id: Option<PolicyActId>,
    pub about_crime_case_id: Option<CrimeCaseId>,
    pub truth_score: i32,
    pub distortion: i32,
    pub credibility_seed: i32,
    pub known_by: Vec<u64>,
    pub origin_day: u32,
    pub origin_tick: u32,
    pub last_spread_tick: u64,
    pub spread_count: u32,
    pub is_slander: bool,
    pub is_confirmed: bool,
    pub is_disproven: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RumorBelief {
    pub rumor_id: u64,
    pub belief: i32,
    pub skepticism: i32,
    pub heard_from: Option<u64>,
    pub first_heard_tick: u64,
    pub last_reinforced_tick: u64,
}

pub type CulturalStoryId = u64;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CulturalStoryKind {
    Lenda,
    HistoriaFamiliar,
    CantoDeGuerra,
    Martirio,
    Milagre,
    Assombracao,
    Fundacao,
    Traicao,
    Heroismo,
    AdvertenciaMoral,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StoryStatus {
    Emergente,
    Estavel,
    Canonizada,
    Contestada,
    Esquecida,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CulturalStory {
    pub id: CulturalStoryId,
    pub title: String,
    pub narrative_core: String,
    pub origin_kind: CulturalStoryKind,
    pub theme: String,
    pub moral: String,
    pub cited_agent_ids: Vec<u64>,
    pub associated_building_id: Option<BuildingId>,
    pub associated_territory_id: Option<TerritoryId>,
    pub source_event_summaries: Vec<String>,
    pub origin_generation: u32,
    pub cultural_strength: i32,
    pub stability: i32,
    pub distortion: i32,
    pub status: StoryStatus,
    pub created_day: u32,
    pub last_told_tick: u64,
    pub tell_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryVersion {
    pub id: u64,
    pub story_id: CulturalStoryId,
    pub short_version: String,
    pub author_agent_id: Option<u64>,
    pub transmitter_agent_id: Option<u64>,
    pub generation: u32,
    pub tone: String,
    pub distortion: i32,
    pub cultural_tags: Vec<String>,
    pub created_day: u32,
    pub created_tick: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoryBelief {
    pub story_id: CulturalStoryId,
    pub belief: i32,
    pub emotional_attachment: i32,
    pub moral_interpretation: String,
    pub heard_from: Option<u64>,
    pub first_heard_tick: u64,
    pub last_heard_tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CulturalTradition {
    pub id: u64,
    pub story_id: CulturalStoryId,
    pub name: String,
    pub associated_building_id: Option<BuildingId>,
    pub associated_faction_id: Option<PoliticalFactionId>,
    pub recurrence_days: u32,
    pub strength: i32,
    pub last_observed_day: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscrowAccount {
    pub id: u64,
    pub depositor_id: u64,
    pub target_agent_id: u64,
    pub resource_id: String,
    pub amount: i32,
    pub condition_secret_id: u64,
}
