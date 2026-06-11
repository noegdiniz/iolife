use crate::world_model::{
    EconomyCatalog, EstablishmentTypeDef, ExternalMarketRule, LocationKind, OwnerPolicyDef,
    RecipeDef, RecipeInputDef, ResourceDef, ResourceKind, ResourceStack, Role, RoleDef,
    SeedAgentDef, SpatialArchetypeDef,
};
use anyhow::{Result, anyhow};
use std::collections::HashSet;

pub fn default_economy_catalog() -> EconomyCatalog {
    EconomyCatalog {
        version: 1,
        resources: vec![
            resource(
                ResourceKind::Graos.id(),
                "Graos",
                &["food", "raw_material"],
                2,
                30,
                true,
                true,
            ),
            resource(
                ResourceKind::Lenha.id(),
                "Lenha",
                &["raw_material", "fuel"],
                2,
                200,
                true,
                true,
            ),
            resource(
                ResourceKind::MetalBruto.id(),
                "Metal Bruto",
                &["raw_material"],
                5,
                210,
                true,
                true,
            ),
            resource(
                ResourceKind::Pao.id(),
                "Pao",
                &["food", "consumable"],
                4,
                10,
                false,
                true,
            ),
            resource(
                ResourceKind::Caldo.id(),
                "Caldo",
                &["food", "consumable"],
                5,
                5,
                false,
                true,
            ),
            resource(
                ResourceKind::Ferramentas.id(),
                "Ferramentas",
                &["capital"],
                9,
                300,
                true,
                true,
            ),
            resource(
                ResourceKind::Moedas.id(),
                "Moedas",
                &["currency"],
                1,
                1000,
                false,
                false,
            ),
        ],
        roles: vec![
            role(
                Role::Farmer.id(),
                Role::Farmer.as_str(),
                &["fazenda", "lenhal", "pedreira"],
            ),
            role(Role::Blacksmith.id(), Role::Blacksmith.as_str(), &["forja"]),
            role(Role::Baker.id(), Role::Baker.as_str(), &["padaria"]),
            role(
                Role::TavernKeeper.id(),
                Role::TavernKeeper.as_str(),
                &["taverna"],
            ),
            role(Role::Guard.id(), Role::Guard.as_str(), &["posto_guarda"]),
            role(Role::Headman.id(), Role::Headman.as_str(), &["solar"]),
        ],
        spatial_archetypes: vec![
            archetype("casa", "Casa", LocationKind::Home),
            archetype("solar", "Solar", LocationKind::Manor),
            archetype("posto_guarda", "Posto da Guarda", LocationKind::GuardPost),
            archetype("forja", "Oficina", LocationKind::Workshop),
            archetype("padaria", "Padaria", LocationKind::Bakery),
            archetype("taverna", "Taverna", LocationKind::Tavern),
            archetype("fazenda", "Campo", LocationKind::Farm),
            archetype("lenhal", "Lenhal", LocationKind::Woodlot),
            archetype("pedreira", "Pedreira", LocationKind::Quarry),
        ],
        establishment_types: vec![
            EstablishmentTypeDef {
                id: "fazenda".to_string(),
                display_name: "Fazenda".to_string(),
                spatial_archetype_id: "fazenda".to_string(),
                location_kind: LocationKind::Farm,
                public_service: false,
                owner_policy: OwnerPolicyDef::SharedByRoles {
                    role_ids: vec![Role::Farmer.id().to_string()],
                },
                wage_per_shift: 5,
                stock_targets: vec![
                    stack(ResourceKind::Graos.id(), 22),
                    stack(ResourceKind::Ferramentas.id(), 2),
                ],
                default_stock: vec![stack(ResourceKind::Ferramentas.id(), 2)],
                production_recipe_id: Some("colheita_graos".to_string()),
            },
            EstablishmentTypeDef {
                id: "lenhal".to_string(),
                display_name: "Lenhal".to_string(),
                spatial_archetype_id: "lenhal".to_string(),
                location_kind: LocationKind::Woodlot,
                public_service: false,
                owner_policy: OwnerPolicyDef::SharedByRoles {
                    role_ids: vec![Role::Farmer.id().to_string()],
                },
                wage_per_shift: 5,
                stock_targets: vec![stack(ResourceKind::Lenha.id(), 18)],
                default_stock: vec![],
                production_recipe_id: Some("coleta_lenha".to_string()),
            },
            EstablishmentTypeDef {
                id: "pedreira".to_string(),
                display_name: "Pedreira".to_string(),
                spatial_archetype_id: "pedreira".to_string(),
                location_kind: LocationKind::Quarry,
                public_service: false,
                owner_policy: OwnerPolicyDef::SharedByRoles {
                    role_ids: vec![Role::Farmer.id().to_string()],
                },
                wage_per_shift: 6,
                stock_targets: vec![stack(ResourceKind::MetalBruto.id(), 14)],
                default_stock: vec![],
                production_recipe_id: Some("extracao_metal".to_string()),
            },
            EstablishmentTypeDef {
                id: "forja".to_string(),
                display_name: "Forja".to_string(),
                spatial_archetype_id: "forja".to_string(),
                location_kind: LocationKind::Workshop,
                public_service: false,
                owner_policy: OwnerPolicyDef::PrivateByRole {
                    role_id: Role::Blacksmith.id().to_string(),
                },
                wage_per_shift: 6,
                stock_targets: vec![
                    stack(ResourceKind::MetalBruto.id(), 8),
                    stack(ResourceKind::Lenha.id(), 7),
                    stack(ResourceKind::Ferramentas.id(), 5),
                ],
                default_stock: vec![
                    stack(ResourceKind::MetalBruto.id(), 5),
                    stack(ResourceKind::Lenha.id(), 5),
                ],
                production_recipe_id: Some("forja_ferramentas".to_string()),
            },
            EstablishmentTypeDef {
                id: "padaria".to_string(),
                display_name: "Padaria".to_string(),
                spatial_archetype_id: "padaria".to_string(),
                location_kind: LocationKind::Bakery,
                public_service: false,
                owner_policy: OwnerPolicyDef::PrivateByRole {
                    role_id: Role::Baker.id().to_string(),
                },
                wage_per_shift: 5,
                stock_targets: vec![
                    stack(ResourceKind::Graos.id(), 12),
                    stack(ResourceKind::Lenha.id(), 6),
                    stack(ResourceKind::Pao.id(), 14),
                ],
                default_stock: vec![stack(ResourceKind::Lenha.id(), 5)],
                production_recipe_id: Some("assar_pao".to_string()),
            },
            EstablishmentTypeDef {
                id: "taverna".to_string(),
                display_name: "Taverna".to_string(),
                spatial_archetype_id: "taverna".to_string(),
                location_kind: LocationKind::Tavern,
                public_service: false,
                owner_policy: OwnerPolicyDef::PrivateByRole {
                    role_id: Role::TavernKeeper.id().to_string(),
                },
                wage_per_shift: 5,
                stock_targets: vec![
                    stack(ResourceKind::Graos.id(), 8),
                    stack(ResourceKind::Lenha.id(), 6),
                    stack(ResourceKind::Caldo.id(), 12),
                ],
                default_stock: vec![stack(ResourceKind::Lenha.id(), 5)],
                production_recipe_id: Some("preparar_caldo".to_string()),
            },
            EstablishmentTypeDef {
                id: "posto_guarda".to_string(),
                display_name: "Posto da Guarda".to_string(),
                spatial_archetype_id: "posto_guarda".to_string(),
                location_kind: LocationKind::GuardPost,
                public_service: true,
                owner_policy: OwnerPolicyDef::Civic,
                wage_per_shift: 4,
                stock_targets: vec![],
                default_stock: vec![],
                production_recipe_id: None,
            },
            EstablishmentTypeDef {
                id: "solar".to_string(),
                display_name: "Solar".to_string(),
                spatial_archetype_id: "solar".to_string(),
                location_kind: LocationKind::Manor,
                public_service: true,
                owner_policy: OwnerPolicyDef::Civic,
                wage_per_shift: 5,
                stock_targets: vec![],
                default_stock: vec![],
                production_recipe_id: None,
            },
        ],
        recipes: vec![
            recipe(
                "colheita_graos",
                "fazenda",
                ResourceKind::Graos.id(),
                6,
                vec![],
                vec![input(ResourceKind::Ferramentas.id(), 1)],
                7,
                4,
                75,
            ),
            recipe(
                "coleta_lenha",
                "lenhal",
                ResourceKind::Lenha.id(),
                5,
                vec![],
                vec![],
                7,
                0,
                55,
            ),
            recipe(
                "extracao_metal",
                "pedreira",
                ResourceKind::MetalBruto.id(),
                3,
                vec![],
                vec![],
                8,
                0,
                55,
            ),
            recipe(
                "forja_ferramentas",
                "forja",
                ResourceKind::Ferramentas.id(),
                2,
                vec![
                    input(ResourceKind::MetalBruto.id(), 1),
                    input(ResourceKind::Lenha.id(), 1),
                ],
                vec![],
                8,
                0,
                55,
            ),
            recipe(
                "assar_pao",
                "padaria",
                ResourceKind::Pao.id(),
                5,
                vec![
                    input(ResourceKind::Graos.id(), 2),
                    input(ResourceKind::Lenha.id(), 1),
                ],
                vec![],
                7,
                0,
                80,
            ),
            recipe(
                "preparar_caldo",
                "taverna",
                ResourceKind::Caldo.id(),
                4,
                vec![
                    input(ResourceKind::Graos.id(), 1),
                    input(ResourceKind::Lenha.id(), 1),
                ],
                vec![],
                7,
                0,
                80,
            ),
        ],
        external_market_rules: vec![
            quote(ResourceKind::Lenha.id(), 3, 1),
            quote(ResourceKind::MetalBruto.id(), 7, 4),
            quote(ResourceKind::Graos.id(), 3, 1),
            quote(ResourceKind::Pao.id(), 5, 2),
            quote(ResourceKind::Caldo.id(), 6, 2),
            quote(ResourceKind::Ferramentas.id(), 10, 6),
        ],
        seeded_agents: vec![
            seed_agent(1, "Alda", Role::Farmer.id()),
            seed_agent(2, "Breno", Role::Blacksmith.id()),
            seed_agent(3, "Celia", Role::Baker.id()),
            seed_agent(4, "Dario", Role::TavernKeeper.id()),
            seed_agent(5, "Elina", Role::Guard.id()),
            seed_agent(6, "Faro", Role::Headman.id()),
            seed_agent(7, "Gisa", Role::Farmer.id()),
            seed_agent(8, "Helmo", Role::Guard.id()),
            seed_agent(9, "Iria", Role::Baker.id()),
            seed_agent(10, "Joran", Role::Farmer.id()),
            seed_agent(11, "Kelda", Role::TavernKeeper.id()),
            seed_agent(12, "Lute", Role::Blacksmith.id()),
        ],
    }
}

pub fn validate_catalog(catalog: &EconomyCatalog) -> Result<()> {
    unique_ids(
        "resource",
        catalog.resources.iter().map(|item| item.id.as_str()),
    )?;
    unique_ids("role", catalog.roles.iter().map(|item| item.id.as_str()))?;
    unique_ids(
        "spatial_archetype",
        catalog
            .spatial_archetypes
            .iter()
            .map(|item| item.id.as_str()),
    )?;
    unique_ids(
        "establishment_type",
        catalog
            .establishment_types
            .iter()
            .map(|item| item.id.as_str()),
    )?;
    unique_ids(
        "recipe",
        catalog.recipes.iter().map(|item| item.id.as_str()),
    )?;

    let resource_ids = catalog
        .resources
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let role_ids = catalog
        .roles
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let archetype_ids = catalog
        .spatial_archetypes
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let establishment_type_ids = catalog
        .establishment_types
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();

    for resource in &catalog.resources {
        if resource.base_price < 0 {
            return Err(anyhow!(
                "resource `{}` has negative base_price",
                resource.id
            ));
        }
    }
    for recipe in &catalog.recipes {
        if !establishment_type_ids.contains(recipe.establishment_type_id.as_str()) {
            return Err(anyhow!(
                "recipe `{}` references unknown establishment_type `{}`",
                recipe.id,
                recipe.establishment_type_id
            ));
        }
        if !resource_ids.contains(recipe.output_resource_id.as_str()) {
            return Err(anyhow!(
                "recipe `{}` references unknown output resource `{}`",
                recipe.id,
                recipe.output_resource_id
            ));
        }
        for input in recipe
            .inputs
            .iter()
            .chain(recipe.capital_requirements.iter())
        {
            if !resource_ids.contains(input.resource_id.as_str()) {
                return Err(anyhow!(
                    "recipe `{}` references unknown input resource `{}`",
                    recipe.id,
                    input.resource_id
                ));
            }
        }
    }
    for establishment in &catalog.establishment_types {
        if !archetype_ids.contains(establishment.spatial_archetype_id.as_str()) {
            return Err(anyhow!(
                "establishment_type `{}` references unknown spatial_archetype `{}`",
                establishment.id,
                establishment.spatial_archetype_id
            ));
        }
        if let Some(recipe_id) = &establishment.production_recipe_id {
            let recipe = catalog
                .recipes
                .iter()
                .find(|item| &item.id == recipe_id)
                .ok_or_else(|| {
                    anyhow!(
                        "establishment_type `{}` references unknown recipe `{}`",
                        establishment.id,
                        recipe_id
                    )
                })?;
            if recipe.establishment_type_id != establishment.id {
                return Err(anyhow!(
                    "establishment_type `{}` recipe `{}` points to a different establishment type",
                    establishment.id,
                    recipe.id
                ));
            }
        }
        for stack in establishment
            .stock_targets
            .iter()
            .chain(establishment.default_stock.iter())
        {
            if !resource_ids.contains(stack.resource_id.as_str()) {
                return Err(anyhow!(
                    "establishment_type `{}` references unknown stock resource `{}`",
                    establishment.id,
                    stack.resource_id
                ));
            }
        }
        match &establishment.owner_policy {
            OwnerPolicyDef::PrivateByRole { role_id } => {
                if !role_ids.contains(role_id.as_str()) {
                    return Err(anyhow!(
                        "establishment_type `{}` references unknown owner role `{}`",
                        establishment.id,
                        role_id
                    ));
                }
            }
            OwnerPolicyDef::SharedByRoles { role_ids: ids } => {
                for role_id in ids {
                    if !role_ids.contains(role_id.as_str()) {
                        return Err(anyhow!(
                            "establishment_type `{}` references unknown shared owner role `{}`",
                            establishment.id,
                            role_id
                        ));
                    }
                }
            }
            OwnerPolicyDef::Civic => {}
        }
    }
    for quote in &catalog.external_market_rules {
        if !resource_ids.contains(quote.resource_id.as_str()) {
            return Err(anyhow!(
                "external market rule references unknown resource `{}`",
                quote.resource_id
            ));
        }
    }
    for agent in &catalog.seeded_agents {
        if !role_ids.contains(agent.role_id.as_str()) {
            return Err(anyhow!(
                "seed agent `{}` references unknown role `{}`",
                agent.name,
                agent.role_id
            ));
        }
    }
    Ok(())
}

fn unique_ids<'a>(label: &str, ids: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let mut seen = HashSet::new();
    for id in ids {
        if !seen.insert(id.to_string()) {
            return Err(anyhow!("duplicate {label} id `{id}`"));
        }
    }
    Ok(())
}

fn resource(
    id: &str,
    display_name: &str,
    tags: &[&str],
    base_price: i32,
    consumption_priority: i32,
    can_buy_external: bool,
    can_sell_external: bool,
) -> ResourceDef {
    ResourceDef {
        id: id.to_string(),
        display_name: display_name.to_string(),
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        base_price,
        consumption_priority,
        can_buy_external,
        can_sell_external,
    }
}

fn role(id: &str, display_name: &str, allowed: &[&str]) -> RoleDef {
    RoleDef {
        id: id.to_string(),
        display_name: display_name.to_string(),
        allowed_establishment_type_ids: allowed.iter().map(|item| item.to_string()).collect(),
        can_take_logistics_tasks: true,
        can_collect_payments: true,
    }
}

fn archetype(id: &str, display_name: &str, location_kind: LocationKind) -> SpatialArchetypeDef {
    SpatialArchetypeDef {
        id: id.to_string(),
        display_name: display_name.to_string(),
        location_kind,
    }
}

fn input(resource_id: &str, amount: i32) -> RecipeInputDef {
    RecipeInputDef {
        resource_id: resource_id.to_string(),
        amount,
    }
}

fn recipe(
    id: &str,
    establishment_type_id: &str,
    output_resource_id: &str,
    output_amount: i32,
    inputs: Vec<RecipeInputDef>,
    capital_requirements: Vec<RecipeInputDef>,
    labor_cost: i32,
    tool_wear: i32,
    priority: u8,
) -> RecipeDef {
    RecipeDef {
        id: id.to_string(),
        establishment_type_id: establishment_type_id.to_string(),
        output_resource_id: output_resource_id.to_string(),
        output_amount,
        inputs,
        capital_requirements,
        labor_cost,
        tool_wear,
        priority,
    }
}

fn quote(resource_id: &str, buy_price: i32, sell_price: i32) -> ExternalMarketRule {
    ExternalMarketRule {
        resource_id: resource_id.to_string(),
        buy_price,
        sell_price,
    }
}

fn stack(resource_id: &str, amount: i32) -> ResourceStack {
    ResourceStack {
        resource_id: resource_id.to_string(),
        amount,
    }
}

fn seed_agent(id: u64, name: &str, role_id: &str) -> SeedAgentDef {
    SeedAgentDef {
        id,
        name: name.to_string(),
        role_id: role_id.to_string(),
    }
}
