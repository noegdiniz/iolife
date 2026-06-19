use super::*;
use crate::world_model::{
    ItemCombatStats, JusticeSeverity, PolicyDomain, RecipeDef, RelationDelta, ResourceStack,
    SentenceKind, SocialMove, TileCoord,
};
use anyhow::anyhow;
use std::collections::HashMap;

pub(super) fn merge_stack(stacks: &mut Vec<ResourceStack>, stack: ResourceStack) {
    if let Some(existing) = stacks
        .iter_mut()
        .find(|existing| existing.resource_id == stack.resource_id)
    {
        existing.amount += stack.amount;
    } else {
        stacks.push(stack);
    }
}

pub(super) fn consume_matching(stacks: &mut Vec<ResourceStack>, accepted: &[&str]) -> bool {
    for stack in stacks.iter_mut() {
        if accepted.contains(&stack.resource_id.as_str()) && stack.amount > 0 {
            stack.amount -= 1;
            return true;
        }
    }
    false
}

pub(super) fn reconstruct_path(
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

pub(super) fn invert_delta(delta: &RelationDelta) -> RelationDelta {
    RelationDelta {
        trust: delta.trust / 2,
        friendship: delta.friendship / 2,
        resentment: delta.resentment,
        attraction: delta.attraction / 2,
        moral_debt: -delta.moral_debt,
        reputation: delta.reputation / 2,
    }
}

pub(super) fn sentence_for_case_severity(
    justice_severity: JusticeSeverity,
    severity: u8,
) -> SentenceKind {
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

pub(super) fn political_issue_summary(
    domain: PolicyDomain,
    proposed_value: &str,
    agenda_tag: &str,
) -> String {
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

pub(super) fn social_goal_from_move(move_kind: SocialMove) -> &'static str {
    match move_kind {
        SocialMove::Chat => "medir o humor do outro",
        SocialMove::Gossip => "trocar rumores uteis",
        SocialMove::TellStory => "transmitir uma historia cultural relevante",
        SocialMove::Promise => "firmar um compromisso",
        SocialMove::Offend => "pressionar e descarregar frustracao",
        SocialMove::Reconcile => "reparar a relacao",
        SocialMove::Favor => "oferecer ajuda e aproximacao",
    }
}

pub(super) fn other_participant(participants: &[u64; 2], current: u64) -> u64 {
    if participants[0] == current {
        participants[1]
    } else {
        participants[0]
    }
}

pub(super) fn extend_summary(current: &str, addition: &str) -> String {
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

impl Simulation {
    pub(super) fn role_display_name(&self, role_id: &str) -> String {
        self.catalog
            .roles
            .iter()
            .find(|role| role.id == role_id)
            .map(|role| role.display_name.clone())
            .unwrap_or_else(|| role_id.to_string())
    }

    pub(super) fn resource_display_name(&self, resource_id: &str) -> String {
        self.catalog
            .resources
            .iter()
            .find(|resource| resource.id == resource_id)
            .map(|resource| resource.display_name.clone())
            .unwrap_or_else(|| resource_id.to_string())
    }

    pub(super) fn item_instance(&self, item_id: ItemInstanceId) -> Option<&ItemInstance> {
        self.item_instances.iter().find(|item| item.id == item_id)
    }

    pub(super) fn item_instance_mut(
        &mut self,
        item_id: ItemInstanceId,
    ) -> Option<&mut ItemInstance> {
        self.item_instances
            .iter_mut()
            .find(|item| item.id == item_id)
    }

    pub(super) fn item_class_for_resource(&self, resource_id: &str) -> Option<ItemClass> {
        self.resource_def(resource_id)
            .and_then(|resource| resource.item_class)
    }

    pub(super) fn is_equipment_resource(&self, resource_id: &str) -> bool {
        self.item_class_for_resource(resource_id).is_some()
    }

    pub(super) fn visible_prestige_summary(&self, agent_id: u64) -> String {
        let score = self.perceived_status_score(agent_id);
        if score <= 5 {
            "parece pobre e gasto".to_string()
        } else if score <= 15 {
            "veste-se de forma simples".to_string()
        } else if score <= 30 {
            "veste-se com sobriedade respeitavel".to_string()
        } else if score <= 50 {
            "parece claramente próspero e bem-apessoado".to_string()
        } else {
            "ostenta refinamento e status visivel".to_string()
        }
    }

    pub(super) fn perceived_status_score(&self, agent_id: u64) -> i32 {
        let Ok(entity) = self.find_agent_entity(agent_id) else {
            return 0;
        };
        let entry = self.world.entity(entity);
        let Some(equipment) = entry.get::<EquipmentComponent>() else {
            return 0;
        };
        equipment
            .0
            .values()
            .filter_map(|item_id| self.item_instance(*item_id))
            .map(|item| {
                let mut score = item.prestige_value;
                if matches!(
                    self.item_class_for_resource(&item.resource_id),
                    Some(ItemClass::Weapon)
                ) {
                    score /= 3;
                }
                score
            })
            .sum::<i32>()
            .clamp(0, 100)
    }

    pub(super) fn equipped_item_summaries(&self, agent_id: u64) -> Vec<String> {
        let mut items = [
            EquipmentSlot::MainHand,
            EquipmentSlot::OffHand,
            EquipmentSlot::Body,
            EquipmentSlot::Outer,
            EquipmentSlot::Accessory1,
            EquipmentSlot::Accessory2,
        ]
        .into_iter()
        .filter_map(|slot| {
            self.equipped_item_in_slot(agent_id, slot)
                .map(|item| format!("{}: {}", slot.as_str(), item.display_name))
        })
        .collect::<Vec<_>>();
        items.sort();
        items
    }

    pub(super) fn visible_equipment_summary(&self, agent_id: u64) -> String {
        let items = self.equipped_item_summaries(agent_id);
        if items.is_empty() {
            "sem equipamento visivel relevante".to_string()
        } else {
            items.join(", ")
        }
    }

    pub(super) fn inventory_item_summaries(&self, agent_id: u64, limit: usize) -> Vec<String> {
        let Ok(entity) = self.find_agent_entity(agent_id) else {
            return Vec::new();
        };
        let entry = self.world.entity(entity);
        let Some(inventory) = entry.get::<ItemInventoryComponent>() else {
            return Vec::new();
        };
        inventory
            .0
            .iter()
            .take(limit)
            .map(|item_id| self.item_display_name_for_id(*item_id))
            .collect()
    }

    pub(super) fn establishment_item_stock_summaries(
        &self,
        building_id: BuildingId,
        limit: usize,
    ) -> Vec<String> {
        self.establishment_by_building(building_id)
            .map(|establishment| {
                establishment
                    .item_stock_ids
                    .iter()
                    .take(limit)
                    .map(|item_id| self.item_display_name_for_id(*item_id))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn item_instance_unit_price(&self, item: &ItemInstance, unit_price_floor: i32) -> i32 {
        let quality_bonus = item.craft_quality_score.clamp(0, 100) / 10;
        let prestige_bonus = item.prestige_value.max(0) / 3;
        let combat_bonus = (item.combat_profile.damage.max(0)
            + item.combat_profile.protection.max(0)
            + item.combat_profile.precision.max(0))
            / 4;
        (unit_price_floor + quality_bonus + prestige_bonus + combat_bonus).max(1)
    }

    pub(super) fn remove_item_from_establishment_stock(
        &mut self,
        establishment_id: EstablishmentId,
        resource_id: &str,
    ) -> Option<ItemInstanceId> {
        let position = self
            .establishment_by_id(establishment_id)?
            .item_stock_ids
            .iter()
            .position(|item_id| {
            self.item_instance(*item_id)
                .map(|item| item.resource_id == resource_id)
                .unwrap_or(false)
        })?;
        Some(
            self.establishment_by_id_mut(establishment_id)?
                .item_stock_ids
                .remove(position),
        )
    }

    pub(super) fn add_item_to_establishment_stock(
        &mut self,
        establishment_id: EstablishmentId,
        item_id: ItemInstanceId,
    ) -> bool {
        let already_present = self
            .establishment_by_id(establishment_id)
            .map(|establishment| establishment.item_stock_ids.contains(&item_id))
            .unwrap_or(false);
        let Some(establishment) = self.establishment_by_id_mut(establishment_id) else {
            return false;
        };
        if !already_present {
            establishment.item_stock_ids.push(item_id);
        }
        let _ = establishment;
        if let Some(item) = self.item_instance_mut(item_id) {
            item.owner_agent_id = None;
            item.owner_household_id = None;
        }
        true
    }

    pub(super) fn add_item_to_agent_inventory(
        &mut self,
        agent_id: u64,
        item_id: ItemInstanceId,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let household_id = self.household_id_for_agent(agent_id);
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut inventory = entity_mut
                .get_mut::<ItemInventoryComponent>()
                .ok_or_else(|| anyhow!("missing item inventory component"))?;
            if !inventory.0.contains(&item_id) {
                inventory.0.push(item_id);
            }
        }
        if let Some(item) = self.item_instance_mut(item_id) {
            item.owner_agent_id = Some(agent_id);
            item.owner_household_id = household_id;
        }
        Ok(())
    }

    pub(super) fn remove_item_from_agent_inventory(
        &mut self,
        agent_id: u64,
        resource_id: &str,
    ) -> Result<Option<ItemInstanceId>> {
        let entity = self.find_agent_entity(agent_id)?;
        let inventory_ids = self
            .world
            .entity(entity)
            .get::<ItemInventoryComponent>()
            .ok_or_else(|| anyhow!("missing item inventory component"))?
            .0
            .clone();
        let selected = inventory_ids.into_iter().find(|item_id| {
            self.item_instance(*item_id)
                .map(|item| item.resource_id == resource_id)
                .unwrap_or(false)
        });
        let Some(item_id) = selected else {
            return Ok(None);
        };
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut inventory = entity_mut
                .get_mut::<ItemInventoryComponent>()
                .ok_or_else(|| anyhow!("missing item inventory component"))?;
            inventory.0.retain(|existing| *existing != item_id);
        }
        {
            let mut entity_mut = self.world.entity_mut(entity);
            let mut equipment = entity_mut
                .get_mut::<EquipmentComponent>()
                .ok_or_else(|| anyhow!("missing equipment component"))?;
            equipment.0.retain(|_, equipped_id| *equipped_id != item_id);
        }
        Ok(Some(item_id))
    }

    pub(super) fn equipped_item_in_slot(
        &self,
        agent_id: u64,
        slot: EquipmentSlot,
    ) -> Option<&ItemInstance> {
        let entity = self.find_agent_entity(agent_id).ok()?;
        let entry = self.world.entity(entity);
        let equipment = entry.get::<EquipmentComponent>()?;
        equipment
            .0
            .get(&slot)
            .and_then(|id| self.item_instance(*id))
    }

    pub(super) fn equipped_weapon_profile(
        &self,
        agent_id: u64,
    ) -> Option<(&ItemInstance, ItemCombatStats)> {
        let item = self.equipped_item_in_slot(agent_id, EquipmentSlot::MainHand)?;
        Some((item, self.current_item_combat_profile(item)))
    }

    pub(super) fn total_armor_protection(&self, agent_id: u64) -> i32 {
        let Ok(entity) = self.find_agent_entity(agent_id) else {
            return 0;
        };
        let entry = self.world.entity(entity);
        let Some(equipment) = entry.get::<EquipmentComponent>() else {
            return 0;
        };
        equipment
            .0
            .values()
            .filter_map(|item_id| self.item_instance(*item_id))
            .filter(|item| {
                matches!(
                    self.item_class_for_resource(&item.resource_id),
                    Some(ItemClass::Armor) | Some(ItemClass::Clothing)
                )
            })
            .map(|item| self.current_item_combat_profile(item).protection)
            .sum::<i32>()
            .clamp(0, 40)
    }

    pub(super) fn current_item_combat_profile(&self, item: &ItemInstance) -> ItemCombatStats {
        let mut profile = item.combat_profile.clone();
        let resource = self.resource_def(&item.resource_id);
        let base_durability = resource.map(|def| def.base_durability).unwrap_or(0).max(1);
        let durability_ratio =
            item.durability.clamp(0, base_durability) as f32 / base_durability as f32;
        let scale = if durability_ratio >= 0.66 {
            1.0
        } else if durability_ratio >= 0.33 {
            0.8
        } else if durability_ratio > 0.0 {
            0.55
        } else {
            0.2
        };
        profile.damage = ((profile.damage as f32) * scale).round() as i32;
        profile.precision = ((profile.precision as f32) * scale).round() as i32;
        profile.protection = ((profile.protection as f32) * scale).round() as i32;
        profile.injury_severity = ((profile.injury_severity as f32) * scale).round() as i32;
        profile
    }

    pub(super) fn item_display_name_for_id(&self, item_id: ItemInstanceId) -> String {
        self.item_instance(item_id)
            .map(|item| item.display_name.clone())
            .unwrap_or_else(|| format!("item#{item_id}"))
    }

    pub(super) fn craft_discipline_for_recipe(&self, recipe: &RecipeDef) -> &'static str {
        let establishment = recipe.establishment_type_id.as_str();
        let resource_id = recipe.output_resource_id.as_str();
        if resource_id.contains("anel")
            || resource_id.contains("broche")
            || resource_id.contains("colar")
            || establishment == "ourivesaria"
        {
            "jewelry"
        } else if resource_id.contains("tunica")
            || resource_id.contains("manto")
            || resource_id.contains("vestido")
            || establishment == "alfaiataria"
        {
            "tailoring"
        } else if resource_id.contains("couro") {
            "leatherwork"
        } else {
            "smithing"
        }
    }

    pub(super) fn craft_proficiency_value(
        &self,
        proficiencies: &CraftProficiencyState,
        discipline: &str,
    ) -> i32 {
        match discipline {
            "tailoring" => proficiencies.tailoring,
            "jewelry" => proficiencies.jewelry,
            "leatherwork" => proficiencies.leatherwork,
            _ => proficiencies.smithing,
        }
    }

    pub(super) fn craft_proficiencies_for_agent(&self, agent_id: u64) -> CraftProficiencyState {
        let Ok(entity) = self.find_agent_entity(agent_id) else {
            return CraftProficiencyState::default();
        };
        self.world
            .entity(entity)
            .get::<CraftProficiencyComponent>()
            .map(|component| component.0.clone())
            .unwrap_or_default()
    }

    pub(super) fn next_item_instance_id(&mut self) -> ItemInstanceId {
        let id = self.next_item_instance_id;
        self.next_item_instance_id += 1;
        id
    }

    pub(super) fn build_item_instance(
        &mut self,
        resource_id: &str,
        maker_agent_id: Option<u64>,
        owner_agent_id: Option<u64>,
        owner_household_id: Option<BuildingId>,
        proficiencies: &CraftProficiencyState,
        recipe: Option<&RecipeDef>,
        material_signature: String,
    ) -> Option<ItemInstance> {
        let resource = self.resource_def(resource_id)?.clone();
        let item_class = resource.item_class?;
        let discipline = recipe
            .map(|entry| self.craft_discipline_for_recipe(entry))
            .unwrap_or(match item_class {
                ItemClass::Clothing => "tailoring",
                ItemClass::Jewelry => "jewelry",
                _ => "smithing",
            });
        let proficiency = self.craft_proficiency_value(proficiencies, discipline);
        let seed = (self.total_ticks as i32
            + maker_agent_id.unwrap_or(0) as i32
            + resource_id.bytes().map(|b| b as i32).sum::<i32>())
            % 11
            - 5;
        let quality = (35 + proficiency / 2 + seed).clamp(0, 100);
        let refinement_level = RefinementLevel::from_quality_score(quality);
        let tier = refinement_level.tier();
        let mut combat_profile = resource.base_combat_stats.clone();
        combat_profile.damage += resource.refinement_scaling.damage_per_tier * tier;
        combat_profile.precision += resource.refinement_scaling.precision_per_tier * tier;
        combat_profile.protection += resource.refinement_scaling.protection_per_tier * tier;
        combat_profile.injury_severity += (tier / 2).max(0);
        let durability = (resource.base_durability
            + resource.refinement_scaling.durability_per_tier * tier)
            .max(1);
        let prestige_value =
            (resource.base_prestige + resource.refinement_scaling.prestige_per_tier * tier).max(0);
        let maker_name_snapshot = maker_agent_id
            .and_then(|agent_id| self.agent_name(agent_id).ok())
            .filter(|name| !name.is_empty());
        Some(ItemInstance {
            id: self.next_item_instance_id(),
            resource_id: resource.id.clone(),
            display_name: format!(
                "{} {}",
                resource.display_name,
                refinement_level.display_name()
            ),
            refinement_level,
            craft_quality_score: quality,
            durability,
            maker_agent_id,
            maker_name_snapshot,
            material_signature,
            combat_profile,
            prestige_value,
            owner_agent_id,
            owner_household_id,
        })
    }

    pub(super) fn maybe_auto_equip_best_items(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let item_ids = self
            .world
            .entity(entity)
            .get::<ItemInventoryComponent>()
            .ok_or_else(|| anyhow!("missing item inventory component"))?
            .0
            .clone();
        let mut chosen = HashMap::new();
        for slot in [
            EquipmentSlot::MainHand,
            EquipmentSlot::OffHand,
            EquipmentSlot::Body,
            EquipmentSlot::Outer,
            EquipmentSlot::Accessory1,
            EquipmentSlot::Accessory2,
        ] {
            let best = item_ids
                .iter()
                .filter_map(|item_id| self.item_instance(*item_id))
                .filter(|item| {
                    self.resource_def(&item.resource_id)
                        .map(|def| def.equip_slot_preferences.contains(&slot))
                        .unwrap_or(false)
                })
                .max_by_key(|item| {
                    let profile = self.current_item_combat_profile(item);
                    profile.damage * 3 + profile.protection * 2 + item.prestige_value
                })
                .map(|item| item.id);
            if let Some(item_id) = best {
                chosen.insert(slot, item_id);
            }
        }
        self.world
            .entity_mut(entity)
            .get_mut::<EquipmentComponent>()
            .ok_or_else(|| anyhow!("missing equipment component"))?
            .0 = chosen;
        Ok(())
    }

    pub(super) fn degrade_item_instance(&mut self, item_id: ItemInstanceId, amount: i32) -> bool {
        if let Some(item) = self.item_instance_mut(item_id) {
            item.durability = (item.durability - amount.max(0)).max(0);
            return item.durability == 0;
        }
        false
    }

    pub(super) fn resource_def(
        &self,
        resource_id: &str,
    ) -> Option<&crate::world_model::ResourceDef> {
        self.catalog
            .resources
            .iter()
            .find(|resource| resource.id == resource_id)
    }

    pub(super) fn resolve_resource_id_from_hint(&self, hint: &str) -> Option<String> {
        let normalized = hint.trim().to_lowercase();
        if normalized.is_empty() {
            return None;
        }
        self.catalog
            .resources
            .iter()
            .find(|resource| {
                resource.id == normalized
                    || normalized.contains(&resource.id)
                    || resource.display_name.to_lowercase() == normalized
                    || normalized.contains(&resource.display_name.to_lowercase())
            })
            .map(|resource| resource.id.clone())
    }

    pub(super) fn role_def(&self, role_id: &str) -> Option<&crate::world_model::RoleDef> {
        self.catalog.roles.iter().find(|role| role.id == role_id)
    }

    pub(super) fn establishment_type_def(
        &self,
        establishment_type_id: &str,
    ) -> Option<&crate::world_model::EstablishmentTypeDef> {
        self.catalog
            .establishment_types
            .iter()
            .find(|entry| entry.id == establishment_type_id)
    }

    pub(super) fn recipe_for_establishment_type(
        &self,
        establishment_type_id: &str,
    ) -> Option<&crate::world_model::RecipeDef> {
        self.recipes_for_establishment_type(establishment_type_id)
            .into_iter()
            .next()
    }

    pub(super) fn recipes_for_establishment_type(
        &self,
        establishment_type_id: &str,
    ) -> Vec<&crate::world_model::RecipeDef> {
        let Some(establishment_type) = self.establishment_type_def(establishment_type_id) else {
            return Vec::new();
        };
        let mut recipe_ids = establishment_type.production_recipe_ids.clone();
        if recipe_ids.is_empty()
            && let Some(recipe_id) = &establishment_type.production_recipe_id
        {
            recipe_ids.push(recipe_id.clone());
        }
        self.catalog
            .recipes
            .iter()
            .filter(|recipe| {
                recipe.establishment_type_id == establishment_type_id
                    && recipe_ids.iter().any(|recipe_id| recipe_id == &recipe.id)
            })
            .collect()
    }

    pub(super) fn recipe_for_establishment(
        &self,
        establishment: &EstablishmentEconomy,
    ) -> Option<&crate::world_model::RecipeDef> {
        self.recipe_for_establishment_type(&establishment.establishment_type_id)
    }

    pub(super) fn recipes_for_establishment(
        &self,
        establishment: &EstablishmentEconomy,
    ) -> Vec<&crate::world_model::RecipeDef> {
        self.recipes_for_establishment_type(&establishment.establishment_type_id)
    }

    pub(super) fn market_quote(
        &self,
        resource_id: &str,
    ) -> Option<&crate::world_model::ExternalMarketQuote> {
        self.village_economy
            .external_quotes
            .iter()
            .find(|quote| quote.resource_id == resource_id)
    }

    pub(super) fn stock_target_amount(
        &self,
        establishment: &EstablishmentEconomy,
        resource_id: &str,
    ) -> i32 {
        establishment
            .stock_targets
            .iter()
            .find(|target| target.resource_id == resource_id)
            .map(|target| target.amount)
            .unwrap_or(0)
    }

    pub(super) fn establishment_item_count(
        &self,
        establishment: &EstablishmentEconomy,
        resource_id: &str,
    ) -> i32 {
        establishment
            .item_stock_ids
            .iter()
            .filter(|item_id| {
                self.item_instance(**item_id)
                    .map(|item| item.resource_id == resource_id)
                    .unwrap_or(false)
            })
            .count() as i32
    }

    pub(super) fn establishment_total_resource_units(
        &self,
        establishment: &EstablishmentEconomy,
        resource_id: &str,
    ) -> i32 {
        let stack_amount = Self::total_resource_amount(&establishment.stock, resource_id);
        if self.is_equipment_resource(resource_id) {
            stack_amount + self.establishment_item_count(establishment, resource_id)
        } else {
            stack_amount
        }
    }

    pub(super) fn is_food_resource(&self, resource_id: &str) -> bool {
        self.resource_def(resource_id)
            .map(|resource| resource.tags.iter().any(|tag| tag == "food"))
            .unwrap_or(false)
    }

    pub(super) fn food_resource_ids_sorted(&self) -> Vec<String> {
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

    pub(super) fn push_event(&mut self, event: WorldEvent) {
        self.events.push(event);
        if self.events.len() > 5_000 {
            let overflow = self.events.len() - 5_000;
            self.events.drain(0..overflow);
        }
    }

    pub(super) fn has_recent_event<F>(&self, within_ticks: u64, predicate: F) -> bool
    where
        F: Fn(&WorldEvent) -> bool,
    {
        let current_tick = self.total_ticks;
        self.events.iter().rev().any(|event| {
            let event_tick = (event.day.saturating_sub(1) as u64 * self.ticks_per_day as u64)
                + event.tick as u64;
            current_tick.saturating_sub(event_tick) <= within_ticks && predicate(event)
        })
    }

    pub(super) fn add_memory(
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

    pub fn find_agent_entity(&self, agent_id: u64) -> Result<Entity> {
        self.world
            .iter_entities()
            .find_map(|entity_ref| {
                entity_ref
                    .get::<AgentCore>()
                    .and_then(|core| (core.id == agent_id).then_some(entity_ref.id()))
            })
            .ok_or_else(|| anyhow!("agent {agent_id} not found"))
    }

    pub(super) fn agent_name(&self, agent_id: u64) -> Result<String> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .name
            .clone())
    }

    pub(super) fn agent_role_id(&self, agent_id: u64) -> Result<String> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .role_id
            .clone())
    }

    pub(super) fn agent_home_building_id(&self, agent_id: u64) -> Result<Option<BuildingId>> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .home_building_id)
    }

    pub(super) fn agent_initial(&self, agent_id: u64) -> Option<char> {
        self.agent_name(agent_id)
            .ok()
            .and_then(|name| name.chars().next())
            .map(|ch| ch.to_ascii_uppercase())
    }

    pub fn agent_state(&mut self, agent_id: u64) -> Result<AgentState> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<StateComponent>()
            .ok_or_else(|| anyhow!("missing state component"))?
            .0
            .clone())
    }

    pub(super) fn agent_profile(&mut self, agent_id: u64) -> Result<AgentProfile> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<ProfileComponent>()
            .ok_or_else(|| anyhow!("missing profile component"))?
            .0
            .clone())
    }

    pub fn agent_injury(&mut self, agent_id: u64) -> Result<InjuryState> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<InjuryComponent>()
            .ok_or_else(|| anyhow!("missing injury component"))?
            .0
            .clone())
    }

    pub(super) fn agent_memories(&mut self, agent_id: u64) -> Result<Vec<AgentMemory>> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<MemoryComponent>()
            .ok_or_else(|| anyhow!("missing memory component"))?
            .0
            .clone())
    }

    pub(super) fn psychological_state_for_agent(
        &mut self,
        agent_id: u64,
    ) -> Result<PsychologicalState> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<PsychologicalStateComponent>()
            .map(|component| component.0.clone())
            .unwrap_or_default())
    }

    pub(super) fn relation_between(&mut self, agent_id: u64, other_id: u64) -> AgentRelation {
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

    pub(super) fn apply_relation_delta(
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

    pub(super) fn household_id_for_agent_immutable(&self, agent_id: u64) -> Option<BuildingId> {
        let entity = self.find_agent_entity(agent_id).ok()?;
        self.world
            .entity(entity)
            .get::<AgentCore>()
            .and_then(|core| core.home_building_id)
    }
}
