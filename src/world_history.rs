use crate::sim_core::SimulationConfig;
use crate::world_model::{
    CulturalStoryKind, EconomyCatalog, HistoricalBootstrapSummary, InsurrectionStage,
    JusticeSeverity, LocalNorms, RationingPolicy, ResourceStack, WarStage,
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;

const FALLBACK_VILLAGE_NAMES: &[&str] = &["Vale Verde", "Pedra Ruiva", "Monte Frio"];
const FALLBACK_NAMES: &[&str] = &[
    "Alda",
    "Breno",
    "Celia",
    "Dario",
    "Elina",
    "Faro",
    "Gisa",
    "Helmo",
    "Iria",
    "Joran",
    "Kelda",
    "Lute",
    "Martim",
    "Nuno",
    "Olga",
    "Pedro",
    "Quelia",
    "Rui",
    "Sancha",
    "Tomas",
    "Ugo",
    "Vasco",
    "Ximena",
    "Zaria",
    "Afonso",
    "Beatriz",
    "Constanca",
    "Duarte",
    "Estevao",
    "Filipa",
    "Goncalo",
    "Henrique",
    "Ines",
    "Joao",
    "Leonor",
    "Manuel",
    "Mafalda",
];
const TRAITS_POOL: &[&[&str]] = &[
    &["observador", "teimoso"],
    &["generoso", "cauteloso"],
    &["trabalhador", "orgulhoso"],
    &["curioso", "desconfiado"],
    &["impulsivo", "ambicioso"],
    &["astuto", "ressentido"],
    &["covarde", "oportunista"],
    &["violento", "leal"],
];
const VALUES_POOL: &[&[&str]] = &[
    &["honra", "sobrevivencia"],
    &["familia", "comunidade"],
    &["riqueza", "justica"],
    &["poder", "vinganca"],
    &["liberdade", "prazer"],
];
const FEARS_POOL: &[&[&str]] = &[
    &["escassez", "humilhacao"],
    &["solidao", "doenca"],
    &["violencia", "fracasso"],
    &["traicao", "irrelevancia"],
    &["aprisionamento", "impotencia"],
];
const TERRITORY_KEYS: &[&str] = &["vila_central", "campos", "lenhal", "pedreira", "civico"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoricalEventKind {
    Demography,
    Scarcity,
    Commerce,
    Construction,
    Succession,
    Decree,
    FeudalObligation,
    CrimeAndJustice,
    FactionalConflict,
    WarImpact,
    CulturalTransmission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoricalFeudalDutyKind {
    Tribute,
    Corvee,
    Levy,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalWorldState {
    pub years_simulated: u32,
    pub foundation_year: i32,
    pub settlements: Vec<HistoricalSettlement>,
    pub wars: Vec<HistoricalWarRecord>,
    pub summary: HistoricalBootstrapSummary,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalSettlement {
    pub id: usize,
    pub name: String,
    pub households: Vec<HistoricalHousehold>,
    pub people: Vec<HistoricalPerson>,
    pub active_establishments: HashMap<String, u32>,
    pub territory_states: Vec<HistoricalTerritoryState>,
    pub polity: HistoricalPolityState,
    pub local_norms: LocalNorms,
    pub story_seeds: Vec<HistoricalStorySeed>,
    pub ledger: Vec<HistoricalLedgerEvent>,
    pub recent_pressures: Vec<HistoricalPressureSeed>,
    pub recent_policy_tags: Vec<String>,
    pub recent_decrees: Vec<HistoricalDecreeSeed>,
    pub recent_feudal_duties: Vec<HistoricalFeudalDutySeed>,
    pub recent_justice_cases: Vec<HistoricalJusticeSeed>,
    pub recent_constructions: Vec<HistoricalConstructionSeed>,
    pub recent_insurrection: Option<HistoricalInsurrectionSeed>,
    pub recent_military_demands: Vec<HistoricalMilitaryDemandSeed>,
    pub leader_person_id: Option<u64>,
    pub captain_person_id: Option<u64>,
    pub field_vassal_person_id: Option<u64>,
    pub steward_person_id: Option<u64>,
    pub recent_succession: Option<HistoricalSuccessionSeed>,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalHousehold {
    pub id: u64,
    pub name: String,
    pub settlement_id: usize,
    pub member_ids: Vec<u64>,
    pub wealth: i32,
    pub grain: i32,
    pub wood: i32,
    pub ore: i32,
    pub social_rank: i32,
    pub rage: i32,
    pub feudal_arrears: i32,
    pub hardship: i32,
    pub legitimacy: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalPerson {
    pub id: u64,
    pub name: String,
    pub household_id: u64,
    pub sex: String,
    pub birth_year: i32,
    pub death_year: Option<i32>,
    pub father_id: Option<u64>,
    pub mother_id: Option<u64>,
    pub spouse_id: Option<u64>,
    pub children_ids: Vec<u64>,
    pub traits: Vec<String>,
    pub values: Vec<String>,
    pub fears: Vec<String>,
    pub leadership: i32,
    pub martial: i32,
    pub craft: i32,
    pub sociability: i32,
    pub diligence: i32,
    pub legitimacy: i32,
    pub trauma: i32,
    pub alive: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalTerritoryState {
    pub key: String,
    pub stability: i32,
    pub strategic_value: i32,
    pub productivity: i32,
    pub controller_settlement_id: usize,
    pub pressure: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalPolityState {
    pub id: u64,
    pub name: String,
    pub treasury: i32,
    pub military_readiness: i32,
    pub ruling_household_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalStorySeed {
    pub title: String,
    pub summary: String,
    pub moral: String,
    pub kind: CulturalStoryKind,
    pub tags: Vec<String>,
    pub cited_names: Vec<String>,
    pub origin_generation: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalLedgerEvent {
    pub kind: HistoricalEventKind,
    pub year: i32,
    pub summary: String,
    pub importance: i32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalPressureSeed {
    pub agenda_tag: String,
    pub proposed_value: String,
    pub intensity: i32,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalDecreeSeed {
    pub agenda_tag: String,
    pub proposed_value: String,
    pub summary: String,
    pub target_territory_key: Option<String>,
    pub legitimacy: i32,
    pub enforcement: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalFeudalDutySeed {
    pub kind: HistoricalFeudalDutyKind,
    pub household_id: u64,
    pub amount: i32,
    pub complied: bool,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalJusticeSeed {
    pub summary: String,
    pub severity: u8,
    pub suspect_household_id: Option<u64>,
    pub victim_household_id: Option<u64>,
    pub proven: bool,
    pub punitive: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalConstructionSeed {
    pub establishment_type_id: String,
    pub target_territory_key: String,
    pub completed: bool,
    pub summary: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalInsurrectionSeed {
    pub agenda_tag: String,
    pub target_territory_key: String,
    pub stage: InsurrectionStage,
    pub popular_support: i32,
    pub repression: i32,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalMilitaryDemandSeed {
    pub stage: WarStage,
    pub required: Vec<ResourceStack>,
    pub cash_required: i32,
    pub target_territory_key: String,
    pub shortage_score: i32,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalSuccessionSeed {
    pub claimant_person_ids: Vec<u64>,
    pub recognized_heir_id: Option<u64>,
    pub legitimacy_gap: i32,
    pub conflict_score: i32,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoricalWarRecord {
    pub attacker_settlement_id: usize,
    pub defender_settlement_id: usize,
    pub attacker_score: i32,
    pub defender_score: i32,
    pub stage: WarStage,
    pub winner_settlement_id: Option<usize>,
    pub started_year: i32,
    pub ended_year: Option<i32>,
    pub summary: String,
}

pub(crate) fn simulate_world_history(
    config: &SimulationConfig,
    catalog: &EconomyCatalog,
) -> HistoricalWorldState {
    let history_seed = config.history_seed.unwrap_or(config.world_seed);
    let mut rng = StdRng::seed_from_u64(history_seed);
    let settlement_count = config.num_villages.max(1).min(3);
    let foundation_year = 1000i32;
    let founding_households = config.history_founding_households.max(3).min(5);
    let mut next_household_id = 1u64;
    let mut next_person_id = 1u64;
    let mut settlements = Vec::new();

    for settlement_idx in 0..settlement_count {
        let name = if settlement_idx == 0 {
            config.village_name.clone()
        } else {
            FALLBACK_VILLAGE_NAMES[(settlement_idx - 1) % FALLBACK_VILLAGE_NAMES.len()].to_string()
        };
        let polity_id = settlement_idx as u64 + 1;
        let polity_name = format!("Dominio de {}", name);
        let mut households = Vec::new();
        let mut people = Vec::new();
        let mut story_seeds = vec![HistoricalStorySeed {
            title: format!("A fundacao de {}", name),
            summary: format!(
                "As primeiras casas de {} abriram caminho entre fome e teimosia.",
                name
            ),
            moral: "A ordem local nasceu de sobrevivencia, disputa e persistencia.".to_string(),
            kind: CulturalStoryKind::Fundacao,
            tags: vec!["fundacao".to_string(), "origem".to_string()],
            cited_names: Vec::new(),
            origin_generation: 0,
        }];
        let mut ledger = vec![HistoricalLedgerEvent {
            kind: HistoricalEventKind::Construction,
            year: foundation_year,
            summary: format!("{} foi fundada por {} casas.", name, founding_households),
            importance: 12,
            tags: vec!["fundacao".to_string()],
        }];
        let mut active_establishments = HashMap::new();
        active_establishments.insert("fazenda".to_string(), 1);
        active_establishments.insert("lenhal".to_string(), 1);
        active_establishments.insert("pedreira".to_string(), 1);

        for household_idx in 0..founding_households {
            let household_id = next_household_id;
            next_household_id += 1;
            let household_name = format!(
                "Casa {}",
                pick_name(household_idx + settlement_idx, catalog)
            );
            let mut member_ids = Vec::new();
            let founder_a = build_person(
                next_person_id,
                household_id,
                foundation_year - rng.random_range(18..=32),
                if household_idx % 2 == 0 {
                    "Masculino"
                } else {
                    "Feminino"
                },
                household_idx + settlement_idx,
                catalog,
                &mut rng,
            );
            next_person_id += 1;
            let founder_b = build_person(
                next_person_id,
                household_id,
                foundation_year - rng.random_range(18..=30),
                if founder_a.sex == "Masculino" {
                    "Feminino"
                } else {
                    "Masculino"
                },
                household_idx + settlement_idx + 11,
                catalog,
                &mut rng,
            );
            next_person_id += 1;

            member_ids.push(founder_a.id);
            member_ids.push(founder_b.id);
            let founder_a_id = founder_a.id;
            let founder_b_id = founder_b.id;
            people.push(founder_a);
            people.push(founder_b);
            set_spouses(&mut people, founder_a_id, founder_b_id);
            if rng.random_bool(0.45) {
                let child_id = next_person_id;
                next_person_id += 1;
                let mut child = build_person(
                    child_id,
                    household_id,
                    foundation_year - rng.random_range(0..=10),
                    if rng.random_bool(0.5) {
                        "Masculino"
                    } else {
                        "Feminino"
                    },
                    household_idx + settlement_idx + 27,
                    catalog,
                    &mut rng,
                );
                child.father_id = Some(
                    if people.iter().find(|p| p.id == founder_a_id).unwrap().sex == "Masculino" {
                        founder_a_id
                    } else {
                        founder_b_id
                    },
                );
                child.mother_id = Some(
                    if people.iter().find(|p| p.id == founder_a_id).unwrap().sex == "Feminino" {
                        founder_a_id
                    } else {
                        founder_b_id
                    },
                );
                register_child(&mut people, founder_a_id, child_id);
                register_child(&mut people, founder_b_id, child_id);
                member_ids.push(child_id);
                people.push(child);
            }
            households.push(HistoricalHousehold {
                id: household_id,
                name: household_name,
                settlement_id: settlement_idx,
                member_ids,
                wealth: rng.random_range(45..=85),
                grain: rng.random_range(18..=34),
                wood: rng.random_range(10..=22),
                ore: rng.random_range(8..=18),
                social_rank: rng.random_range(10..=24),
                rage: 0,
                feudal_arrears: 0,
                hardship: 0,
                legitimacy: rng.random_range(35..=60),
            });
        }

        settlements.push(HistoricalSettlement {
            id: settlement_idx,
            name: name.clone(),
            households,
            people,
            active_establishments,
            territory_states: TERRITORY_KEYS
                .iter()
                .enumerate()
                .map(|(idx, key)| HistoricalTerritoryState {
                    key: (*key).to_string(),
                    stability: 55 + idx as i32 * 4,
                    strategic_value: match *key {
                        "vila_central" => 32,
                        "campos" => 48,
                        "lenhal" => 26,
                        "pedreira" => 28,
                        "civico" => 36,
                        _ => 20,
                    },
                    productivity: 12 + idx as i32 * 3,
                    controller_settlement_id: settlement_idx,
                    pressure: 45,
                })
                .collect(),
            polity: HistoricalPolityState {
                id: polity_id,
                name: polity_name,
                treasury: 60,
                military_readiness: 18,
                ruling_household_id: None,
            },
            local_norms: LocalNorms::default(),
            story_seeds: std::mem::take(&mut story_seeds),
            ledger: std::mem::take(&mut ledger),
            recent_pressures: Vec::new(),
            recent_policy_tags: Vec::new(),
            recent_decrees: Vec::new(),
            recent_feudal_duties: Vec::new(),
            recent_justice_cases: Vec::new(),
            recent_constructions: Vec::new(),
            recent_insurrection: None,
            recent_military_demands: Vec::new(),
            leader_person_id: None,
            captain_person_id: None,
            field_vassal_person_id: None,
            steward_person_id: None,
            recent_succession: None,
        });
    }

    let mut world = HistoricalWorldState {
        years_simulated: config.history_years.max(1),
        foundation_year,
        settlements,
        wars: Vec::new(),
        summary: HistoricalBootstrapSummary::default(),
    };

    for offset in 0..world.years_simulated {
        let year = foundation_year + offset as i32;
        for settlement in &mut world.settlements {
            settlement.recent_pressures.clear();
            settlement.recent_policy_tags.clear();
            settlement.recent_decrees.clear();
            settlement.recent_feudal_duties.clear();
            settlement.recent_justice_cases.clear();
            settlement.recent_constructions.clear();
            settlement.recent_military_demands.clear();
            settlement.recent_insurrection = None;
            simulate_spring(settlement, year, catalog, &mut next_person_id, &mut rng);
            simulate_summer(settlement, year, &mut rng);
            simulate_autumn(settlement, year, &mut rng);
        }
        simulate_winter(&mut world, year, &mut rng);
    }

    for settlement in &mut world.settlements {
        finalize_settlement_roles(settlement);
    }
    world.summary = build_summary(&world, founding_households);
    world
}

fn simulate_spring(
    settlement: &mut HistoricalSettlement,
    year: i32,
    catalog: &EconomyCatalog,
    next_person_id: &mut u64,
    rng: &mut StdRng,
) {
    let unmarried: Vec<u64> = settlement
        .people
        .iter()
        .filter(|person| person.alive && age_at(person, year) >= 18 && person.spouse_id.is_none())
        .map(|person| person.id)
        .collect();

    let mut pending_marriages = Vec::new();
    for pair in unmarried.chunks(2) {
        if pair.len() == 2 && rng.random_bool(0.35) {
            let a = settlement
                .people
                .iter()
                .find(|person| person.id == pair[0])
                .unwrap();
            let b = settlement
                .people
                .iter()
                .find(|person| person.id == pair[1])
                .unwrap();
            if a.sex != b.sex && a.household_id != b.household_id {
                pending_marriages.push((a.id, b.id));
            }
        }
    }
    for (a, b) in pending_marriages {
        set_spouses(&mut settlement.people, a, b);
        settlement.ledger.push(HistoricalLedgerEvent {
            kind: HistoricalEventKind::Demography,
            year,
            summary: format!(
                "{} e {} firmaram casamento entre linhagens.",
                person_name(&settlement.people, a),
                person_name(&settlement.people, b)
            ),
            importance: 5,
            tags: vec!["casamento".to_string()],
        });
    }

    let fertile_women = settlement
        .people
        .iter()
        .filter(|person| {
            person.alive
                && person.sex == "Feminino"
                && person.spouse_id.is_some()
                && (18..=40).contains(&age_at(person, year))
        })
        .map(|person| person.id)
        .collect::<Vec<_>>();
    for mother_id in fertile_women {
        let mother = settlement
            .people
            .iter()
            .find(|person| person.id == mother_id)
            .unwrap();
        let spouse_id = mother.spouse_id.expect("fertile woman has spouse");
        let household_index = settlement
            .households
            .iter()
            .position(|household| household.id == mother.household_id)
            .expect("mother household exists");
        let household = &settlement.households[household_index];
        let fertility_boost = if household.grain > household.member_ids.len() as i32 * 3 {
            0.22
        } else {
            0.08
        };
        if rng.random_bool(fertility_boost) {
            let child_id = *next_person_id;
            *next_person_id += 1;
            let mut child = build_person(
                child_id,
                mother.household_id,
                year,
                if rng.random_bool(0.5) {
                    "Masculino"
                } else {
                    "Feminino"
                },
                child_id as usize,
                catalog,
                rng,
            );
            let father_id = if mother.sex == "Feminino" {
                spouse_id
            } else {
                mother_id
            };
            child.mother_id = Some(mother_id);
            child.father_id = Some(father_id);
            settlement.people.push(child);
            register_child(&mut settlement.people, mother_id, child_id);
            register_child(&mut settlement.people, spouse_id, child_id);
            settlement.households[household_index]
                .member_ids
                .push(child_id);
            settlement.ledger.push(HistoricalLedgerEvent {
                kind: HistoricalEventKind::Demography,
                year,
                summary: format!(
                    "Nasceu {} na {}.",
                    person_name(&settlement.people, child_id),
                    settlement.households[household_index].name
                ),
                importance: 4,
                tags: vec!["nascimento".to_string()],
            });
        }
    }
}

fn simulate_summer(settlement: &mut HistoricalSettlement, year: i32, rng: &mut StdRng) {
    let adult_ids = living_adults(settlement, year);
    let mut total_grain = 0;
    let mut total_wood = 0;
    let mut total_ore = 0;
    let mut craft_pressure = 0;
    for household in &mut settlement.households {
        let members = household
            .member_ids
            .iter()
            .filter(|member_id| {
                settlement
                    .people
                    .iter()
                    .find(|person| person.id == **member_id)
                    .is_some_and(|person| person.alive)
            })
            .count() as i32;
        let household_adults = household
            .member_ids
            .iter()
            .filter_map(|member_id| {
                settlement
                    .people
                    .iter()
                    .find(|person| person.id == *member_id)
            })
            .filter(|person| person.alive && age_at(person, year) >= 16)
            .collect::<Vec<_>>();
        let diligence: i32 = household_adults
            .iter()
            .map(|person| person.diligence)
            .sum::<i32>()
            / household_adults.len().max(1) as i32;
        let craft: i32 = household_adults
            .iter()
            .map(|person| person.craft)
            .sum::<i32>()
            / household_adults.len().max(1) as i32;
        let grain_gain = members.max(1) * 2
            + diligence / 18
            + settlement
                .active_establishments
                .get("fazenda")
                .copied()
                .unwrap_or(0) as i32
            + rng.random_range(0..=3);
        let wood_gain = members.max(1)
            + settlement
                .active_establishments
                .get("lenhal")
                .copied()
                .unwrap_or(0) as i32
            + rng.random_range(0..=2);
        let ore_gain = (members / 2).max(1)
            + settlement
                .active_establishments
                .get("pedreira")
                .copied()
                .unwrap_or(0) as i32
            + rng.random_range(0..=2);
        household.grain += grain_gain;
        household.wood += wood_gain;
        household.ore += ore_gain;
        household.wealth += grain_gain / 2 + craft / 20;
        total_grain += grain_gain;
        total_wood += wood_gain;
        total_ore += ore_gain;
        craft_pressure += craft;
    }

    let mut new_constructions = Vec::new();
    if adult_ids.len() >= 5
        && total_grain >= 18
        && !settlement.active_establishments.contains_key("padaria")
    {
        settlement
            .active_establishments
            .insert("padaria".to_string(), 1);
        new_constructions.push((
            "padaria",
            "vila_central",
            "A producao de graos sustentou a abertura de uma padaria.",
            "deficit de alimento processado",
        ));
    }
    if adult_ids.len() >= 5
        && total_grain >= 15
        && settlement.households.iter().any(|h| h.wealth >= 55)
        && !settlement.active_establishments.contains_key("taverna")
    {
        settlement
            .active_establishments
            .insert("taverna".to_string(), 1);
        new_constructions.push((
            "taverna",
            "vila_central",
            "O excedente e a riqueza local sustentaram a primeira taverna.",
            "circulacao comercial e social",
        ));
    }
    if total_ore >= 10
        && total_wood >= 10
        && craft_pressure / adult_ids.len().max(1) as i32 >= 40
        && !settlement.active_establishments.contains_key("forja")
    {
        settlement
            .active_establishments
            .insert("forja".to_string(), 1);
        new_constructions.push((
            "forja",
            "vila_central",
            "A abundancia de metal e madeira permitiu erguer uma forja.",
            "capacidade artesanal e metalurgica",
        ));
    }
    if adult_ids.len() >= 4
        && !settlement
            .active_establishments
            .contains_key("posto_guarda")
    {
        settlement
            .active_establishments
            .insert("posto_guarda".to_string(), 1);
        new_constructions.push((
            "posto_guarda",
            "civico",
            "A inseguranca e o crescimento local exigiram um posto da guarda.",
            "controle civico e coercao",
        ));
    }
    if settlement
        .households
        .iter()
        .any(|household| household.wealth >= 70)
        && !settlement.active_establishments.contains_key("solar")
    {
        settlement
            .active_establishments
            .insert("solar".to_string(), 1);
        new_constructions.push((
            "solar",
            "civico",
            "A concentracao de riqueza consolidou um solar senhorial.",
            "consolidacao de poder local",
        ));
    }
    for (establishment_type_id, target_territory_key, summary, reason) in new_constructions {
        settlement
            .recent_constructions
            .push(HistoricalConstructionSeed {
                establishment_type_id: establishment_type_id.to_string(),
                target_territory_key: target_territory_key.to_string(),
                completed: true,
                summary: summary.to_string(),
                reason: reason.to_string(),
            });
        settlement.ledger.push(HistoricalLedgerEvent {
            kind: HistoricalEventKind::Construction,
            year,
            summary: summary.to_string(),
            importance: 9,
            tags: vec![
                "construcao".to_string(),
                establishment_type_id.to_string(),
                target_territory_key.to_string(),
            ],
        });
    }
    let commerce_gain = (total_grain / 8) + (total_wood / 10) + (total_ore / 10);
    settlement.polity.treasury += commerce_gain;
    if commerce_gain >= 6 {
        settlement.ledger.push(HistoricalLedgerEvent {
            kind: HistoricalEventKind::Commerce,
            year,
            summary: format!(
                "{} trocou excedentes e reforcou o caixa publico em {} moedas agregadas.",
                settlement.name, commerce_gain
            ),
            importance: 6,
            tags: vec!["comercio".to_string(), "tributo".to_string()],
        });
    }
}

fn simulate_autumn(settlement: &mut HistoricalSettlement, year: i32, rng: &mut StdRng) {
    let mut total_rage = 0;
    for household in &mut settlement.households {
        let living = household
            .member_ids
            .iter()
            .filter(|member_id| {
                settlement
                    .people
                    .iter()
                    .find(|person| person.id == **member_id)
                    .is_some_and(|person| person.alive)
            })
            .count() as i32;
        let food_required = living.max(1) * 3;
        let mut shortage = 0;
        if household.grain >= food_required {
            household.grain -= food_required;
        } else {
            shortage = food_required - household.grain;
            household.grain = 0;
            let external_cost = shortage * 2;
            if household.wealth >= external_cost {
                household.wealth -= external_cost;
            } else {
                let unpaid = external_cost - household.wealth;
                household.wealth = 0;
                shortage += unpaid / 2;
            }
        }

        let tax_due = if settlement.polity.military_readiness >= 35 {
            3
        } else {
            2
        };
        let paid = household.wealth.min(tax_due);
        household.wealth -= paid;
        settlement.polity.treasury += paid;
        settlement
            .recent_feudal_duties
            .push(HistoricalFeudalDutySeed {
                kind: HistoricalFeudalDutyKind::Tribute,
                household_id: household.id,
                amount: tax_due,
                complied: paid >= tax_due,
                summary: if paid >= tax_due {
                    format!(
                        "{} pagou o tributo integral de {}.",
                        household.name, tax_due
                    )
                } else {
                    format!(
                        "{} pagou apenas {} de {} em tributo.",
                        household.name, paid, tax_due
                    )
                },
            });
        if paid < tax_due {
            settlement.ledger.push(HistoricalLedgerEvent {
                kind: HistoricalEventKind::FeudalObligation,
                year,
                summary: format!(
                    "{} atrasou parte do tributo senhorial em {}.",
                    household.name, settlement.name
                ),
                importance: 5,
                tags: vec!["tributo".to_string(), "inadimplencia".to_string()],
            });
        }
        if paid < tax_due {
            household.feudal_arrears += tax_due - paid;
            household.rage += 6 + (tax_due - paid);
        } else {
            household.feudal_arrears = (household.feudal_arrears - 1).max(0);
        }

        if shortage > 0 {
            household.hardship += shortage;
            household.rage += shortage * 2;
            household.legitimacy = (household.legitimacy - shortage).max(-40);
            settlement.ledger.push(HistoricalLedgerEvent {
                kind: HistoricalEventKind::Scarcity,
                year,
                summary: format!(
                    "{} sofreu escassez alimentar sazonal em {}.",
                    household.name, settlement.name
                ),
                importance: 6,
                tags: vec!["fome".to_string(), "escassez".to_string()],
            });
            settlement.recent_pressures.push(HistoricalPressureSeed {
                agenda_tag: "motim_comida".to_string(),
                proposed_value: "aliviar".to_string(),
                intensity: (shortage * 3).clamp(4, 30),
                reason: format!("{} entrou em fome sazonal.", household.name),
            });
            if shortage >= 4 {
                settlement.story_seeds.push(HistoricalStorySeed {
                    title: format!("A fome de {}", settlement.name),
                    summary: format!(
                        "A fome corroeu a autoridade e a paciencia em {}.",
                        settlement.name
                    ),
                    moral: "Despensas vazias corroem qualquer ordem.".to_string(),
                    kind: CulturalStoryKind::AdvertenciaMoral,
                    tags: vec!["fome".to_string(), "escassez".to_string()],
                    cited_names: Vec::new(),
                    origin_generation: (year
                        - settlement
                            .people
                            .iter()
                            .map(|p| p.birth_year)
                            .min()
                            .unwrap_or(year))
                    .max(0) as u32
                        / 20,
                });
            }
        } else if rng.random_bool(0.2) {
            household.legitimacy = (household.legitimacy + 1).min(80);
        }
        total_rage += household.rage;
        if household.wealth >= 25 && rng.random_bool(0.12) {
            settlement
                .recent_feudal_duties
                .push(HistoricalFeudalDutySeed {
                    kind: HistoricalFeudalDutyKind::Corvee,
                    household_id: household.id,
                    amount: 1,
                    complied: true,
                    summary: format!(
                        "{} prestou corveia sazonal em manutencao e obras do dominio.",
                        household.name
                    ),
                });
            settlement.ledger.push(HistoricalLedgerEvent {
                kind: HistoricalEventKind::FeudalObligation,
                year,
                summary: format!(
                    "{} prestou corveia sazonal para manter obras e rotas do dominio.",
                    household.name
                ),
                importance: 4,
                tags: vec!["corveia".to_string(), "dominio".to_string()],
            });
        }
    }

    let avg_rage = total_rage / settlement.households.len().max(1) as i32;
    let avg_arrears = settlement
        .households
        .iter()
        .map(|household| household.feudal_arrears)
        .sum::<i32>()
        / settlement.households.len().max(1) as i32;
    settlement.local_norms.rationing_policy = if avg_rage >= 35 {
        RationingPolicy::Balanced
    } else if settlement.polity.treasury >= 120 {
        RationingPolicy::CivicFirst
    } else {
        RationingPolicy::Balanced
    };
    settlement.local_norms.justice_severity = if avg_rage >= 40 {
        JusticeSeverity::Severe
    } else if avg_rage <= 12 {
        JusticeSeverity::Lenient
    } else {
        JusticeSeverity::Normal
    };
    if avg_rage >= 34 {
        settlement.recent_decrees.push(HistoricalDecreeSeed {
            agenda_tag: "racionamento_estrito".to_string(),
            proposed_value: "estrito".to_string(),
            summary: format!(
                "{} endureceu o racionamento para conter a fome e a desordem.",
                settlement.name
            ),
            target_territory_key: Some("vila_central".to_string()),
            legitimacy: (50 - avg_rage / 2).clamp(-30, 70),
            enforcement: 55,
        });
        settlement.ledger.push(HistoricalLedgerEvent {
            kind: HistoricalEventKind::Decree,
            year,
            summary: format!(
                "{} endureceu o racionamento para preservar a ordem alimentar.",
                settlement.name
            ),
            importance: 8,
            tags: vec!["decreto".to_string(), "racionamento".to_string()],
        });
    }
    if avg_arrears >= 2 || settlement.polity.military_readiness >= 35 {
        settlement.recent_decrees.push(HistoricalDecreeSeed {
            agenda_tag: "imposto_guerra".to_string(),
            proposed_value: "elevar".to_string(),
            summary: format!(
                "{} elevou a cobranca senhorial para sustentar caixa e preparacao militar.",
                settlement.name
            ),
            target_territory_key: Some("civico".to_string()),
            legitimacy: (30 - avg_arrears * 5).clamp(-40, 60),
            enforcement: 60,
        });
        settlement.ledger.push(HistoricalLedgerEvent {
            kind: HistoricalEventKind::Decree,
            year,
            summary: format!(
                "{} reforcou imposto de guerra e cobrancas atrasadas.",
                settlement.name
            ),
            importance: 7,
            tags: vec!["decreto".to_string(), "imposto".to_string()],
        });
    }
    if avg_rage >= 24 && rng.random_bool(0.35) {
        let suspect = settlement
            .households
            .iter()
            .max_by_key(|household| household.rage)
            .map(|household| household.id);
        let victim = settlement
            .households
            .iter()
            .min_by_key(|household| household.wealth)
            .map(|household| household.id);
        let punitive = settlement
            .active_establishments
            .contains_key("posto_guarda");
        settlement.recent_justice_cases.push(HistoricalJusticeSeed {
            summary: if punitive {
                format!(
                    "Uma punicao exemplar seguiu suspeitas de furto e desordem em {}.",
                    settlement.name
                )
            } else {
                format!(
                    "Furtos e vingancas privadas cresceram em {} sem resposta institucional forte.",
                    settlement.name
                )
            },
            severity: if punitive { 6 } else { 4 },
            suspect_household_id: suspect,
            victim_household_id: victim,
            proven: punitive,
            punitive,
        });
        settlement.ledger.push(HistoricalLedgerEvent {
            kind: HistoricalEventKind::CrimeAndJustice,
            year,
            summary: settlement
                .recent_justice_cases
                .last()
                .map(|seed| seed.summary.clone())
                .unwrap_or_else(|| "Conflito de ordem e justica.".to_string()),
            importance: 6,
            tags: vec!["crime".to_string(), "justica".to_string()],
        });
    }
}

fn simulate_winter(world: &mut HistoricalWorldState, year: i32, rng: &mut StdRng) {
    for settlement in &mut world.settlements {
        let old_leader = settlement.leader_person_id;
        let mut deaths = Vec::new();
        for person in &mut settlement.people {
            if !person.alive {
                continue;
            }
            let age = age_at(person, year);
            let household = settlement
                .households
                .iter()
                .find(|household| household.id == person.household_id)
                .unwrap();
            let mut death_chance = if age >= 80 {
                0.45
            } else if age >= 65 {
                0.16
            } else {
                0.01
            };
            death_chance += (household.hardship as f64 * 0.008).clamp(0.0, 0.25);
            death_chance += (person.trauma as f64 * 0.0015).clamp(0.0, 0.08);
            if rng.random_bool(death_chance.clamp(0.0, 0.85)) {
                person.alive = false;
                person.death_year = Some(year);
                deaths.push(person.id);
            } else if household.hardship > 0 {
                person.trauma = (person.trauma + household.hardship).clamp(0, 100);
            }
        }
        for death_id in deaths {
            settlement.ledger.push(HistoricalLedgerEvent {
                kind: HistoricalEventKind::Demography,
                year,
                summary: format!(
                    "{} morreu durante o inverno.",
                    person_name(&settlement.people, death_id)
                ),
                importance: 7,
                tags: vec!["morte".to_string(), "inverno".to_string()],
            });
        }

        finalize_settlement_roles(settlement);
        if old_leader != settlement.leader_person_id {
            let mut claimant_ids = top_adults_by_score(settlement, year, 3, |person, household| {
                person.legitimacy + person.leadership + household.social_rank + household.wealth / 5
            });
            claimant_ids.truncate(3);
            let legitimacy_gap = claimant_ids
                .get(0)
                .zip(claimant_ids.get(1))
                .map(|(a, b)| {
                    person_score(settlement, *a, year) - person_score(settlement, *b, year)
                })
                .unwrap_or(18);
            let summary = match (old_leader, settlement.leader_person_id) {
                (Some(prev), Some(next)) => format!(
                    "{} perdeu o comando para {}.",
                    person_name(&settlement.people, prev),
                    person_name(&settlement.people, next)
                ),
                (None, Some(next)) => format!(
                    "{} consolidou a primeira autoridade dominante de {}.",
                    person_name(&settlement.people, next),
                    settlement.name
                ),
                _ => format!("{} entrou em vacancia de autoridade.", settlement.name),
            };
            settlement.recent_succession = Some(HistoricalSuccessionSeed {
                claimant_person_ids: claimant_ids.clone(),
                recognized_heir_id: settlement.leader_person_id,
                legitimacy_gap,
                conflict_score: (28 - legitimacy_gap).max(0),
                summary: summary.clone(),
            });
            settlement.ledger.push(HistoricalLedgerEvent {
                kind: HistoricalEventKind::Succession,
                year,
                summary: summary.clone(),
                importance: 10,
                tags: vec!["sucessao".to_string()],
            });
            settlement.story_seeds.push(HistoricalStorySeed {
                title: format!("A sucessao de {}", settlement.name),
                summary,
                moral: "Poder sem herdeiro claro convida disputa.".to_string(),
                kind: CulturalStoryKind::Traicao,
                tags: vec!["sucessao".to_string(), "poder".to_string()],
                cited_names: claimant_ids
                    .iter()
                    .map(|id| person_name(&settlement.people, *id))
                    .collect(),
                origin_generation: 1,
            });
        }

        let avg_rage = settlement.households.iter().map(|h| h.rage).sum::<i32>()
            / settlement.households.len().max(1) as i32;
        let avg_arrears = settlement
            .households
            .iter()
            .map(|household| household.feudal_arrears)
            .sum::<i32>()
            / settlement.households.len().max(1) as i32;
        if avg_rage >= 26 {
            settlement.recent_pressures.push(HistoricalPressureSeed {
                agenda_tag: "boicote_imposto".to_string(),
                proposed_value: "reduzir".to_string(),
                intensity: avg_rage.clamp(8, 30),
                reason: format!("{} terminou o ano com raiva acumulada.", settlement.name),
            });
        }
        if avg_arrears >= 2 {
            settlement
                .recent_policy_tags
                .push("imposto_guerra".to_string());
        }
        if avg_rage >= 34 && settlement.local_norms.justice_severity != JusticeSeverity::Severe {
            settlement
                .recent_policy_tags
                .push("racionamento_estrito".to_string());
            settlement.local_norms.justice_severity = JusticeSeverity::Severe;
        }
        let avg_hardship = settlement
            .households
            .iter()
            .map(|household| household.hardship)
            .sum::<i32>()
            / settlement.households.len().max(1) as i32;
        if avg_rage >= 30 || avg_hardship >= 5 {
            let stage = if avg_rage >= 52 {
                InsurrectionStage::CivilWar
            } else if avg_rage >= 42 {
                InsurrectionStage::OrganizedRevolt
            } else if avg_rage >= 34 {
                InsurrectionStage::Riot
            } else {
                InsurrectionStage::Agitation
            };
            let popular_support = (avg_rage * 2 + avg_hardship * 5).clamp(10, 100);
            let repression = if settlement
                .active_establishments
                .contains_key("posto_guarda")
            {
                (settlement.polity.military_readiness + 15).clamp(5, 100)
            } else {
                settlement.polity.military_readiness.clamp(0, 100)
            };
            let agenda_tag = if avg_hardship >= avg_rage / 3 {
                "motim_comida"
            } else {
                "depor_lider"
            };
            settlement.recent_insurrection = Some(HistoricalInsurrectionSeed {
                agenda_tag: agenda_tag.to_string(),
                target_territory_key: "vila_central".to_string(),
                stage,
                popular_support,
                repression,
                summary: format!(
                    "{} entrou em {:?} por fome, tributo e raiva acumulada.",
                    settlement.name, stage
                ),
            });
            settlement.ledger.push(HistoricalLedgerEvent {
                kind: HistoricalEventKind::FactionalConflict,
                year,
                summary: settlement
                    .recent_insurrection
                    .as_ref()
                    .map(|seed| seed.summary.clone())
                    .unwrap_or_else(|| "Conflito faccional.".to_string()),
                importance: 9,
                tags: vec!["revolta".to_string(), agenda_tag.to_string()],
            });
            if repression >= popular_support && stage != InsurrectionStage::Agitation {
                settlement.recent_justice_cases.push(HistoricalJusticeSeed {
                    summary: format!(
                        "A guarda reprimiu violentamente a revolta recente em {}.",
                        settlement.name
                    ),
                    severity: 7,
                    suspect_household_id: None,
                    victim_household_id: None,
                    proven: true,
                    punitive: true,
                });
                settlement.ledger.push(HistoricalLedgerEvent {
                    kind: HistoricalEventKind::CrimeAndJustice,
                    year,
                    summary: format!(
                        "A repressao de {} deixou feridas institucionais e memoria de medo.",
                        settlement.name
                    ),
                    importance: 8,
                    tags: vec!["repressao".to_string(), "justica".to_string()],
                });
            }
        }
    }

    simulate_inter_settlement_war(world, year, rng);
}

fn simulate_inter_settlement_war(world: &mut HistoricalWorldState, year: i32, rng: &mut StdRng) {
    if world.settlements.len() < 2 {
        return;
    }
    if world.wars.is_empty() && rng.random_bool(0.08) {
        let attacker = strongest_settlement(world, year);
        let defender = weakest_other_settlement(world, attacker, year);
        if attacker != defender {
            let attacker_name = world.settlements[attacker].name.clone();
            let defender_name = world.settlements[defender].name.clone();
            world.wars.push(HistoricalWarRecord {
                attacker_settlement_id: attacker,
                defender_settlement_id: defender,
                attacker_score: 20,
                defender_score: 16,
                stage: WarStage::Mobilization,
                winner_settlement_id: None,
                started_year: year,
                ended_year: None,
                summary: format!(
                    "{} iniciou guerra de pressao contra {}.",
                    attacker_name, defender_name
                ),
            });
            world.settlements[attacker]
                .ledger
                .push(HistoricalLedgerEvent {
                    kind: HistoricalEventKind::WarImpact,
                    year,
                    summary: format!(
                        "{} mobilizou recursos para guerra contra {}.",
                        attacker_name, defender_name
                    ),
                    importance: 8,
                    tags: vec!["guerra".to_string(), "mobilizacao".to_string()],
                });
        }
    }

    for war in &mut world.wars {
        if war.ended_year.is_some() {
            continue;
        }
        apply_war_demands(
            &mut world.settlements[war.attacker_settlement_id],
            war.stage,
            true,
            year,
        );
        apply_war_demands(
            &mut world.settlements[war.defender_settlement_id],
            war.stage,
            false,
            year,
        );
        let attacker_power = settlement_power(&world.settlements[war.attacker_settlement_id], year);
        let defender_power = settlement_power(&world.settlements[war.defender_settlement_id], year);
        war.attacker_score +=
            ((attacker_power - defender_power) / 8).max(2) + rng.random_range(0..=6);
        war.defender_score +=
            ((defender_power - attacker_power) / 10).max(1) + rng.random_range(0..=5);
        war.stage = if war.attacker_score.max(war.defender_score) >= 90 {
            WarStage::DecisiveBattle
        } else if war.attacker_score.max(war.defender_score) >= 70 {
            WarStage::Siege
        } else if war.attacker_score.max(war.defender_score) >= 45 {
            WarStage::Raids
        } else {
            WarStage::Mobilization
        };
        if war.attacker_score >= 100 || war.defender_score >= 100 {
            let winner = if war.attacker_score >= war.defender_score {
                war.attacker_settlement_id
            } else {
                war.defender_settlement_id
            };
            let winner_name = world.settlements[winner].name.clone();
            let loser = if winner == war.attacker_settlement_id {
                war.defender_settlement_id
            } else {
                war.attacker_settlement_id
            };
            let loser_name = world.settlements[loser].name.clone();
            war.winner_settlement_id = Some(winner);
            war.ended_year = Some(year);
            war.stage = WarStage::Occupation;
            war.summary = format!("{} venceu a guerra contra {}.", winner_name, loser_name);
            for territory in &mut world.settlements[loser].territory_states {
                if territory.key != "vila_central" && rng.random_bool(0.5) {
                    territory.controller_settlement_id = winner;
                    territory.pressure = 65;
                }
            }
            world.settlements[winner]
                .story_seeds
                .push(HistoricalStorySeed {
                    title: format!("A vitoria sobre {}", loser_name),
                    summary: war.summary.clone(),
                    moral: "A guerra recompensa preparo, mas deixa cicatriz.".to_string(),
                    kind: CulturalStoryKind::CantoDeGuerra,
                    tags: vec!["guerra".to_string(), "vitoria".to_string()],
                    cited_names: Vec::new(),
                    origin_generation: 1,
                });
            world.settlements[loser]
                .story_seeds
                .push(HistoricalStorySeed {
                    title: format!("A derrota perante {}", winner_name),
                    summary: war.summary.clone(),
                    moral: "Derrotas longas alimentam medo e ressentimento.".to_string(),
                    kind: CulturalStoryKind::Martirio,
                    tags: vec!["guerra".to_string(), "derrota".to_string()],
                    cited_names: Vec::new(),
                    origin_generation: 1,
                });
            world.settlements[winner]
                .ledger
                .push(HistoricalLedgerEvent {
                    kind: HistoricalEventKind::WarImpact,
                    year,
                    summary: format!(
                        "{} venceu a guerra e consolidou ocupacao periférica.",
                        winner_name
                    ),
                    importance: 11,
                    tags: vec!["guerra".to_string(), "vitoria".to_string()],
                });
            world.settlements[loser].ledger.push(HistoricalLedgerEvent {
                kind: HistoricalEventKind::WarImpact,
                year,
                summary: format!(
                    "{} perdeu recursos, territorio e legitimidade apos a derrota.",
                    loser_name
                ),
                importance: 11,
                tags: vec!["guerra".to_string(), "derrota".to_string()],
            });
        }
    }
}

fn apply_war_demands(
    settlement: &mut HistoricalSettlement,
    stage: WarStage,
    defending: bool,
    year: i32,
) {
    let (required, cash_required, shortage_score) = match stage {
        WarStage::Mobilization => (
            vec![
                ResourceStack {
                    resource_id: "graos".to_string(),
                    amount: 6,
                },
                ResourceStack {
                    resource_id: "ferramentas".to_string(),
                    amount: 2,
                },
            ],
            8,
            10,
        ),
        WarStage::Raids => (
            vec![
                ResourceStack {
                    resource_id: "graos".to_string(),
                    amount: 8,
                },
                ResourceStack {
                    resource_id: "ferramentas".to_string(),
                    amount: 2,
                },
                ResourceStack {
                    resource_id: "metal_bruto".to_string(),
                    amount: 2,
                },
            ],
            10,
            14,
        ),
        WarStage::Siege => (
            vec![
                ResourceStack {
                    resource_id: "graos".to_string(),
                    amount: 12,
                },
                ResourceStack {
                    resource_id: "madeira".to_string(),
                    amount: 4,
                },
                ResourceStack {
                    resource_id: "pedra".to_string(),
                    amount: 4,
                },
            ],
            14,
            20,
        ),
        WarStage::DecisiveBattle => (
            vec![
                ResourceStack {
                    resource_id: "graos".to_string(),
                    amount: 10,
                },
                ResourceStack {
                    resource_id: "ferramentas".to_string(),
                    amount: 3,
                },
                ResourceStack {
                    resource_id: "metal_bruto".to_string(),
                    amount: 3,
                },
            ],
            16,
            24,
        ),
        WarStage::Occupation => (
            vec![
                ResourceStack {
                    resource_id: "graos".to_string(),
                    amount: 7,
                },
                ResourceStack {
                    resource_id: "madeira".to_string(),
                    amount: 2,
                },
            ],
            9,
            12,
        ),
    };

    let summary = if defending {
        format!(
            "{} sustentou demanda defensiva de {:?} com pressão sobre comida e caixa.",
            settlement.name, stage
        )
    } else {
        format!(
            "{} sustentou demanda ofensiva de {:?} com pressão sobre comida e caixa.",
            settlement.name, stage
        )
    };
    settlement
        .recent_military_demands
        .push(HistoricalMilitaryDemandSeed {
            stage,
            required: required.clone(),
            cash_required,
            target_territory_key: if defending {
                "civico".to_string()
            } else {
                "vila_central".to_string()
            },
            shortage_score,
            summary: summary.clone(),
        });

    let compliance = settlement.polity.treasury >= cash_required;
    settlement
        .recent_feudal_duties
        .push(HistoricalFeudalDutySeed {
            kind: HistoricalFeudalDutyKind::Levy,
            household_id: settlement.polity.ruling_household_id.unwrap_or(0),
            amount: cash_required.max(1),
            complied: compliance,
            summary: if compliance {
                format!(
                    "{} atendeu convocação militar de {:?}.",
                    settlement.name, stage
                )
            } else {
                format!(
                    "{} atrasou convocação militar de {:?} por falta de caixa.",
                    settlement.name, stage
                )
            },
        });

    let treasury_after = settlement.polity.treasury - cash_required;
    if treasury_after >= 0 {
        settlement.polity.treasury = treasury_after;
        settlement.polity.military_readiness =
            (settlement.polity.military_readiness + 4).clamp(0, 100);
    } else {
        let shortage = treasury_after.abs();
        settlement.polity.treasury = 0;
        settlement.polity.military_readiness =
            (settlement.polity.military_readiness - (6 + shortage / 2)).clamp(0, 100);
        for household in &mut settlement.households {
            household.hardship += 1 + shortage / 6;
            household.rage += 2 + shortage / 5;
            household.legitimacy = (household.legitimacy - 2 - shortage / 8).clamp(-80, 100);
        }
        settlement.recent_pressures.push(HistoricalPressureSeed {
            agenda_tag: "boicote_imposto".to_string(),
            proposed_value: "reduzir".to_string(),
            intensity: (8 + shortage).clamp(6, 28),
            reason: format!(
                "{} sofreu custo militar excessivo durante {:?}.",
                settlement.name, stage
            ),
        });
    }

    settlement.ledger.push(HistoricalLedgerEvent {
        kind: HistoricalEventKind::WarImpact,
        year,
        summary,
        importance: match stage {
            WarStage::Mobilization => 6,
            WarStage::Raids => 7,
            WarStage::Siege => 9,
            WarStage::DecisiveBattle => 10,
            WarStage::Occupation => 8,
        },
        tags: vec![
            "guerra".to_string(),
            "suprimento_militar".to_string(),
            if defending {
                "defesa".to_string()
            } else {
                "ofensiva".to_string()
            },
        ],
    });
}

fn finalize_settlement_roles(settlement: &mut HistoricalSettlement) {
    let current_year = settlement
        .people
        .iter()
        .filter_map(|person| {
            if person.alive {
                Some(person.birth_year)
            } else {
                None
            }
        })
        .min()
        .map(|min_birth| min_birth + 100)
        .unwrap_or(1100);
    let household_scores = settlement
        .households
        .iter()
        .map(|household| {
            (
                household.id,
                household_power(settlement, household.id, current_year),
            )
        })
        .collect::<Vec<_>>();
    let leader_household_id = household_scores
        .iter()
        .max_by_key(|(_, score)| *score)
        .map(|(id, _)| *id);
    settlement.polity.ruling_household_id = leader_household_id;
    settlement.leader_person_id = leader_household_id.and_then(|household_id| {
        top_person_in_household(settlement, household_id, current_year, |person| {
            person.leadership + person.legitimacy + person.sociability / 2
        })
    });
    settlement.captain_person_id = top_person_global(settlement, current_year, |person| {
        person.martial + person.legitimacy / 2
    });
    settlement.field_vassal_person_id = top_person_global(settlement, current_year, |person| {
        person.diligence + person.legitimacy / 3
    });
    settlement.steward_person_id = top_person_global(settlement, current_year, |person| {
        person.craft + person.sociability / 2
    });
}

fn build_summary(
    world: &HistoricalWorldState,
    founding_households: usize,
) -> HistoricalBootstrapSummary {
    let living_population = world
        .settlements
        .iter()
        .map(|settlement| {
            settlement
                .people
                .iter()
                .filter(|person| person.alive)
                .count()
        })
        .sum();
    let surviving_households = world
        .settlements
        .iter()
        .map(|settlement| settlement.households.len())
        .sum();
    let major_dynasties = world
        .settlements
        .iter()
        .filter_map(|settlement| {
            settlement
                .polity
                .ruling_household_id
                .and_then(|household_id| {
                    settlement
                        .households
                        .iter()
                        .find(|household| household.id == household_id)
                })
                .map(|household| {
                    format!(
                        "{} domina {} com riqueza={} e rank={}",
                        household.name, settlement.name, household.wealth, household.social_rank
                    )
                })
        })
        .collect::<Vec<_>>();
    let mut major_conflicts = world
        .wars
        .iter()
        .map(|war| war.summary.clone())
        .collect::<Vec<_>>();
    for settlement in &world.settlements {
        if let Some(succession) = &settlement.recent_succession {
            major_conflicts.push(succession.summary.clone());
        }
    }
    major_conflicts.truncate(6);
    let major_foundations = world
        .settlements
        .iter()
        .flat_map(|settlement| settlement.story_seeds.iter().take(2))
        .map(|story| story.title.clone())
        .take(6)
        .collect::<Vec<_>>();
    HistoricalBootstrapSummary {
        years_simulated: world.years_simulated,
        founding_households: founding_households * world.settlements.len(),
        surviving_households,
        living_population,
        major_dynasties,
        major_conflicts,
        major_foundations,
    }
}

fn build_person(
    id: u64,
    household_id: u64,
    birth_year: i32,
    sex: &str,
    pool_index: usize,
    catalog: &EconomyCatalog,
    rng: &mut StdRng,
) -> HistoricalPerson {
    let base_name = pick_name(pool_index, catalog);
    HistoricalPerson {
        id,
        name: format!("{} {}", base_name, (household_id % 97) + 1),
        household_id,
        sex: sex.to_string(),
        birth_year,
        death_year: None,
        father_id: None,
        mother_id: None,
        spouse_id: None,
        children_ids: Vec::new(),
        traits: TRAITS_POOL[pool_index % TRAITS_POOL.len()]
            .iter()
            .map(|value| value.to_string())
            .collect(),
        values: VALUES_POOL[pool_index % VALUES_POOL.len()]
            .iter()
            .map(|value| value.to_string())
            .collect(),
        fears: FEARS_POOL[pool_index % FEARS_POOL.len()]
            .iter()
            .map(|value| value.to_string())
            .collect(),
        leadership: rng.random_range(20..=85),
        martial: rng.random_range(15..=90),
        craft: rng.random_range(15..=90),
        sociability: rng.random_range(15..=90),
        diligence: rng.random_range(25..=95),
        legitimacy: rng.random_range(20..=70),
        trauma: 0,
        alive: true,
    }
}

fn pick_name(index: usize, catalog: &EconomyCatalog) -> String {
    if !catalog.seeded_agents.is_empty() {
        return catalog.seeded_agents[index % catalog.seeded_agents.len()]
            .name
            .clone();
    }
    FALLBACK_NAMES[index % FALLBACK_NAMES.len()].to_string()
}

fn set_spouses(people: &mut [HistoricalPerson], a: u64, b: u64) {
    if let Some(person) = people.iter_mut().find(|person| person.id == a) {
        person.spouse_id = Some(b);
    }
    if let Some(person) = people.iter_mut().find(|person| person.id == b) {
        person.spouse_id = Some(a);
    }
}

fn register_child(people: &mut [HistoricalPerson], parent_id: u64, child_id: u64) {
    if let Some(parent) = people.iter_mut().find(|person| person.id == parent_id) {
        if !parent.children_ids.contains(&child_id) {
            parent.children_ids.push(child_id);
        }
    }
}

fn age_at(person: &HistoricalPerson, current_year: i32) -> i32 {
    (current_year - person.birth_year).max(0)
}

fn person_name(people: &[HistoricalPerson], person_id: u64) -> String {
    people
        .iter()
        .find(|person| person.id == person_id)
        .map(|person| person.name.clone())
        .unwrap_or_else(|| format!("agente {}", person_id))
}

fn living_adults(settlement: &HistoricalSettlement, year: i32) -> Vec<u64> {
    settlement
        .people
        .iter()
        .filter(|person| person.alive && age_at(person, year) >= 16)
        .map(|person| person.id)
        .collect()
}

fn top_person_global(
    settlement: &HistoricalSettlement,
    year: i32,
    scorer: impl Fn(&HistoricalPerson) -> i32,
) -> Option<u64> {
    settlement
        .people
        .iter()
        .filter(|person| person.alive && age_at(person, year) >= 18)
        .max_by_key(|person| scorer(person))
        .map(|person| person.id)
}

fn top_person_in_household(
    settlement: &HistoricalSettlement,
    household_id: u64,
    year: i32,
    scorer: impl Fn(&HistoricalPerson) -> i32,
) -> Option<u64> {
    settlement
        .people
        .iter()
        .filter(|person| {
            person.alive && person.household_id == household_id && age_at(person, year) >= 18
        })
        .max_by_key(|person| scorer(person))
        .map(|person| person.id)
}

fn household_power(settlement: &HistoricalSettlement, household_id: u64, year: i32) -> i32 {
    let Some(household) = settlement
        .households
        .iter()
        .find(|household| household.id == household_id)
    else {
        return 0;
    };
    let living = household
        .member_ids
        .iter()
        .filter_map(|id| settlement.people.iter().find(|person| person.id == *id))
        .filter(|person| person.alive && age_at(person, year) >= 12)
        .count() as i32;
    household.wealth + household.grain + household.social_rank + household.legitimacy + living * 6
        - household.feudal_arrears * 3
        - household.hardship * 2
}

fn person_score(settlement: &HistoricalSettlement, person_id: u64, year: i32) -> i32 {
    let Some(person) = settlement
        .people
        .iter()
        .find(|person| person.id == person_id)
    else {
        return 0;
    };
    let household = settlement
        .households
        .iter()
        .find(|household| household.id == person.household_id);
    person.leadership
        + person.legitimacy
        + person.sociability / 2
        + household
            .map(|household| household.social_rank + household.wealth / 6)
            .unwrap_or(0)
        - person.trauma / 3
        + age_at(person, year) / 2
}

fn top_adults_by_score(
    settlement: &HistoricalSettlement,
    year: i32,
    limit: usize,
    scorer: impl Fn(&HistoricalPerson, &HistoricalHousehold) -> i32,
) -> Vec<u64> {
    let mut ranked = settlement
        .people
        .iter()
        .filter(|person| person.alive && age_at(person, year) >= 18)
        .filter_map(|person| {
            settlement
                .households
                .iter()
                .find(|household| household.id == person.household_id)
                .map(|household| (person.id, scorer(person, household)))
        })
        .collect::<Vec<_>>();
    ranked.sort_by_key(|(_, score)| -*score);
    ranked.into_iter().take(limit).map(|(id, _)| id).collect()
}

fn settlement_power(settlement: &HistoricalSettlement, year: i32) -> i32 {
    let living = settlement
        .people
        .iter()
        .filter(|person| person.alive)
        .count() as i32;
    let wealth = settlement
        .households
        .iter()
        .map(|household| household.wealth)
        .sum::<i32>();
    let readiness = settlement.polity.military_readiness;
    let legitimacy = settlement
        .leader_person_id
        .and_then(|leader_id| {
            settlement
                .people
                .iter()
                .find(|person| person.id == leader_id)
        })
        .map(|leader| leader.legitimacy + leader.leadership / 2)
        .unwrap_or(15);
    living * 3 + wealth / 5 + readiness + legitimacy + (year % 7)
}

fn strongest_settlement(world: &HistoricalWorldState, year: i32) -> usize {
    world
        .settlements
        .iter()
        .enumerate()
        .max_by_key(|(_, settlement)| settlement_power(settlement, year))
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn weakest_other_settlement(world: &HistoricalWorldState, attacker: usize, year: i32) -> usize {
    world
        .settlements
        .iter()
        .enumerate()
        .filter(|(idx, _)| *idx != attacker)
        .min_by_key(|(_, settlement)| settlement_power(settlement, year))
        .map(|(idx, _)| idx)
        .unwrap_or(attacker)
}
