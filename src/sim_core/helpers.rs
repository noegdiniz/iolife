use super::*;
use crate::world_model::{
    JusticeSeverity, PolicyDomain, RelationDelta, ResourceStack, SentenceKind, SocialMove,
    TileCoord,
};
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

    pub(super) fn resource_def(
        &self,
        resource_id: &str,
    ) -> Option<&crate::world_model::ResourceDef> {
        self.catalog
            .resources
            .iter()
            .find(|resource| resource.id == resource_id)
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

    pub(super) fn agent_state(&mut self, agent_id: u64) -> Result<AgentState> {
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
