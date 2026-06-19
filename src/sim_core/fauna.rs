use super::*;
use crate::world_model::{
    AgentLifeStatus, BodyPartKind, CombatOutcome, CombatState, CombatStatus, Creature, EventKind,
    HuntingQuest, InjuryState, MemoryKind, PartInjuryStatus, RelationDelta, ResourceKind,
    ResourceStack, WorldEvent, default_body_parts,
};
use anyhow::{Result, anyhow};
use bevy_ecs::prelude::*;
use std::collections::HashSet;

impl Simulation {
    // Generates a dynamic compound name for magical creatures.
    pub fn generate_creature_name(&self, seed: u64, is_legendary: bool) -> String {
        if is_legendary {
            let names = [
                "Vorax", "Thyra", "Gorgoth", "Zephyrus", "Ignis", "Kaldor", "Valerius", "Morgrim",
                "Eldrin", "Sariel",
            ];
            let epithets = [
                "o Faminto",
                "a Luminosa",
                "o Destruidor",
                "o Evasivo",
                "o Flagelo de Ferro",
                "o Glacial",
                "o Imortal",
                "o Antigo",
                "o Protetor",
                "a Sombra",
            ];
            let name_idx = (seed % names.len() as u64) as usize;
            let ep_idx = ((seed / names.len() as u64) % epithets.len() as u64) as usize;
            format!("{}, {}", names[name_idx], epithets[ep_idx])
        } else {
            let prefixes = [
                "Drak", "Lum", "Aethel", "Sombra", "Giga", "Vora", "Terra", "Aero", "Aqua", "Pyro",
            ];
            let roots = [
                "ignis", "petra", "sylva", "aether", "necro", "glacio", "tempest", "phobia", "lux",
                "ferro",
            ];
            let suffixes = [
                "serpe",
                "corvo",
                "lebre",
                "touro",
                "lobo",
                "urso",
                "felino",
                "aranha",
                "vespa",
                "tartaruga",
            ];

            let p_idx = (seed % prefixes.len() as u64) as usize;
            let r_idx = ((seed / prefixes.len() as u64) % roots.len() as u64) as usize;
            let s_idx =
                ((seed / (prefixes.len() * roots.len()) as u64) % suffixes.len() as u64) as usize;
            format!("{}{}{}", prefixes[p_idx], roots[r_idx], suffixes[s_idx])
        }
    }

    pub fn is_creature(&self, id: u64) -> bool {
        self.world
            .iter_entities()
            .any(|e| e.get::<CreatureCore>().map(|c| c.id == id).unwrap_or(false))
    }

    pub fn find_creature_entity(&self, id: u64) -> Result<Entity> {
        self.world
            .iter_entities()
            .find_map(|e| {
                e.get::<CreatureCore>()
                    .and_then(|c| (c.id == id).then_some(e.id()))
            })
            .ok_or_else(|| anyhow!("Creature {} not found", id))
    }

    pub fn creature_name(&self, id: u64) -> Result<String> {
        let ent = self.find_creature_entity(id)?;
        Ok(self
            .world
            .entity(ent)
            .get::<CreatureCore>()
            .unwrap()
            .name
            .clone())
    }

    pub fn creature_position(&self, id: u64) -> Result<TileCoord> {
        let ent = self.find_creature_entity(id)?;
        Ok(self.world.entity(ent).get::<PositionComponent>().unwrap().0)
    }

    pub fn is_creature_alive(&self, id: u64) -> Result<bool> {
        let ent = self.find_creature_entity(id)?;
        Ok(self
            .world
            .entity(ent)
            .get::<LifeStatusComponent>()
            .unwrap()
            .0
            == AgentLifeStatus::Vivo)
    }

    // Spawns 2 legendary creatures at world generation
    pub fn spawn_initial_legendary_creatures(&mut self) {
        let territories = self.territories.clone();
        if territories.len() < 2 {
            return;
        }

        // Spawn a legendary Sylvapharus in Forest
        let t1_id = territories[0].id;
        let p1 = territories[0]
            .tile_coords
            .first()
            .copied()
            .unwrap_or(TileCoord { x: 5, y: 5 });
        let id1 = self.next_creature_id();
        let name1 = self.generate_creature_name(id1, true);
        self.spawn_creature_entity(
            id1,
            name1,
            "Silvafaro".to_string(),
            true,
            p1,
            t1_id,
            120,
            30,
        );

        // Spawn a legendary Petrapyre in Rock
        let t2_id = territories[1].id;
        let p2 = territories[1]
            .tile_coords
            .first()
            .copied()
            .unwrap_or(TileCoord { x: 10, y: 10 });
        let id2 = self.next_creature_id();
        let name2 = self.generate_creature_name(id2, true);
        self.spawn_creature_entity(
            id2,
            name2,
            "Pedrapiro".to_string(),
            true,
            p2,
            t2_id,
            150,
            45,
        );
    }

    pub fn next_creature_id(&mut self) -> u64 {
        let id = self.next_creature_id;
        self.next_creature_id += 1;
        id
    }

    pub fn spawn_creature_entity(
        &mut self,
        id: u64,
        name: String,
        species: String,
        is_legendary: bool,
        position: TileCoord,
        habitat_territory_id: u64,
        max_health: i32,
        attack_power: i32,
    ) {
        self.world.spawn((
            CreatureCore {
                id,
                name,
                species,
                is_legendary,
                habitat_territory_id,
            },
            CreatureStateComponent {
                health: max_health,
                max_health,
                attack_power,
            },
            InjuryComponent(InjuryState {
                body_parts: default_body_parts(),
                ..Default::default()
            }),
            PositionComponent(position),
            LifeStatusComponent(AgentLifeStatus::Vivo),
            DestinationComponent(None),
            PathComponent(Vec::new()),
        ));
    }

    // Daily spawns up to 10 common creatures if biomes exist
    pub fn spawn_daily_common_creatures(&mut self) {
        let active_count = self
            .world
            .iter_entities()
            .filter(|e| {
                e.get::<CreatureCore>().is_some()
                    && e.get::<LifeStatusComponent>()
                        .map(|l| l.0 == AgentLifeStatus::Vivo)
                        .unwrap_or(false)
            })
            .count();

        if active_count >= 10 {
            return;
        }

        let needed = 10 - active_count;
        let territories = self.territories.clone();
        if territories.is_empty() {
            return;
        }

        let seed = self.total_ticks;
        for i in 0..needed {
            let t_idx = ((seed + i as u64) % territories.len() as u64) as usize;
            let territory = &territories[t_idx];
            let species = match (seed + i as u64) % 4 {
                0 => "Silvafaro",
                1 => "Pedrapiro",
                2 => "Lebre-zelo",
                _ => "Brumalisco",
            };

            let pos = territory
                .tile_coords
                .first()
                .copied()
                .unwrap_or(TileCoord { x: 10, y: 10 });
            let id = self.next_creature_id();
            let name = self.generate_creature_name(id, false);

            let (max_hp, att) = match species {
                "Silvafaro" => (60, 15),
                "Pedrapiro" => (90, 25),
                "Lebre-zelo" => (30, 0),
                _ => (50, 20),
            };

            self.spawn_creature_entity(
                id,
                name,
                species.to_string(),
                false,
                pos,
                territory.id,
                max_hp,
                att,
            );
        }
    }

    // Applies human attack turn on creature
    pub fn apply_attack_on_creature(
        &mut self,
        actor_id: u64,
        target_creature_id: u64,
        continuing_combat: bool,
    ) -> Result<()> {
        let actor_name = self.agent_name(actor_id)?;
        let c_ent = self.find_creature_entity(target_creature_id)?;

        let (creature_died, c_core, target_part) = {
            let mut query = self.world.query::<(
                &CreatureCore,
                &mut CreatureStateComponent,
                &mut InjuryComponent,
                &mut LifeStatusComponent,
            )>();
            let (c_core, mut c_state, mut c_injury, mut c_life) = query
                .get_mut(&mut self.world, c_ent)
                .map_err(|e| anyhow!("Failed to query creature components: {:?}", e))?;

            if c_life.0 != AgentLifeStatus::Vivo {
                return Ok(());
            }

            let base_damage = if continuing_combat { 10 } else { 15 };
            let damage = base_damage;

            // Roll target body part
            let hash = (actor_id ^ target_creature_id ^ self.total_ticks) as usize;
            let roll = hash % 100;
            let target_part = if roll < 30 {
                BodyPartKind::Chest
            } else if roll < 55 {
                BodyPartKind::LeftArm
            } else if roll < 80 {
                BodyPartKind::LeftLeg
            } else if roll < 90 {
                BodyPartKind::Abdomen
            } else if roll < 95 {
                BodyPartKind::Neck
            } else if roll < 99 {
                BodyPartKind::Head
            } else {
                BodyPartKind::Heart
            };

            c_state.health = (c_state.health - damage).max(0);
            let mut creature_died = c_state.health <= 0;

            if let Some(part) = c_injury
                .0
                .body_parts
                .iter_mut()
                .find(|p| p.kind == target_part)
            {
                part.health = (part.health - damage).max(0);
                if part.health <= 0 {
                    part.status = PartInjuryStatus::Destroyed;
                    part.pain = 100;
                    part.bleeding = 6;
                    if matches!(
                        target_part,
                        BodyPartKind::Head | BodyPartKind::Heart | BodyPartKind::Neck
                    ) {
                        creature_died = true;
                    }
                } else {
                    part.status = PartInjuryStatus::Lacerated;
                    part.pain += damage;
                }
            }

            if creature_died {
                c_state.health = 0;
                c_life.0 = AgentLifeStatus::Morto;
            }

            (creature_died, c_core.clone(), target_part)
        };

        let c_name = c_core.name.clone();

        if creature_died {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: actor_id,
                target: Some(target_creature_id),
                kind: EventKind::CreatureKilled,
                summary: format!(
                    "{} derrotou a criatura {} ({})!",
                    actor_name, c_name, c_core.species
                ),
                impact_tags: vec!["combate".to_string(), "caça".to_string()],
            });

            // Deliver magical drop to caçador inventory
            let drop_resource = match c_core.species.as_str() {
                "Silvafaro" => "essencia_silvafaro",
                "Pedrapiro" => "nucleo_pedrapiro",
                "Lebre-zelo" => "pelo_lebre_zelo",
                _ => "bruma_condensada",
            };
            self.give_drop_to_agent(actor_id, drop_resource)?;

            // Resolve Quests & pay gold reward
            self.resolve_hunting_quests_for_creature(actor_id, target_creature_id)?;

            // Handle legendary gossip
            if c_core.is_legendary {
                self.add_memory(
                    actor_id,
                    MemoryKind::Success,
                    format!("Eu derrotei a temivel criatura lendaria {}!", c_name),
                    vec!["heroísmo".to_string(), "lenda".to_string()],
                    50,
                    Vec::new(),
                )?;
                // Generate a cultural story / legend in the village
                self.create_legendary_story(actor_id, &c_name, &c_core.species);
            }

            // End combat states
            self.end_combats_involving(target_creature_id, CombatOutcome::Death);
        } else {
            self.ensure_combat(actor_id, target_creature_id)?;
        }

        Ok(())
    }

    // Applies creature attack turn on agent
    pub fn apply_creature_attack_on_agent(
        &mut self,
        creature_id: u64,
        target_agent_id: u64,
    ) -> Result<()> {
        let c_ent = self.find_creature_entity(creature_id)?;
        let c_entry = self.world.entity(c_ent);
        let c_core = c_entry.get::<CreatureCore>().unwrap().clone();
        let c_state = c_entry.get::<CreatureStateComponent>().unwrap().clone();
        let c_life = c_entry.get::<LifeStatusComponent>().unwrap().0;

        if c_life != AgentLifeStatus::Vivo || c_core.species == "Lebre-zelo" {
            return Ok(()); // Lebre-zelo never attacks
        }

        let a_ent = self.find_agent_entity(target_agent_id)?;
        let c_name = c_core.name.clone();

        let (agent_name, agent_died, target_part, damage) = {
            let mut query = self.world.query::<(
                &mut StateComponent,
                &mut InjuryComponent,
                &mut LifeStatusComponent,
                &AgentCore,
            )>();
            let (mut a_state, mut a_injury, mut a_life, a_core) = query
                .get_mut(&mut self.world, a_ent)
                .map_err(|e| anyhow!("Failed to query components: {:?}", e))?;

            if a_life.0 != AgentLifeStatus::Vivo {
                return Ok(());
            }

            let agent_name = a_core.name.clone();
            let damage = c_state.attack_power;

            // Roll target body part
            let hash = (creature_id ^ target_agent_id ^ self.total_ticks) as usize;
            let roll = hash % 100;
            let target_part = if roll < 30 {
                BodyPartKind::Chest
            } else if roll < 55 {
                BodyPartKind::LeftArm
            } else if roll < 80 {
                BodyPartKind::LeftLeg
            } else if roll < 90 {
                BodyPartKind::Abdomen
            } else if roll < 95 {
                BodyPartKind::Neck
            } else if roll < 99 {
                BodyPartKind::Head
            } else {
                BodyPartKind::Heart
            };

            a_state.0.health = (a_state.0.health - damage).max(0);
            let mut agent_died = a_state.0.health <= 0;

            if let Some(part) = a_injury
                .0
                .body_parts
                .iter_mut()
                .find(|p| p.kind == target_part)
            {
                part.health = (part.health - damage).max(0);
                if part.health <= 0 {
                    part.status = PartInjuryStatus::Destroyed;
                    part.pain = 100;
                    part.bleeding = 6;
                    if matches!(
                        target_part,
                        BodyPartKind::Head | BodyPartKind::Heart | BodyPartKind::Neck
                    ) {
                        agent_died = true;
                    }
                } else {
                    part.status = PartInjuryStatus::Lacerated;
                    part.pain += damage;
                }
            }
            (agent_name, agent_died, target_part, damage)
        };

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: creature_id,
            target: Some(target_agent_id),
            kind: EventKind::Violence,
            summary: format!(
                "A criatura {} atacou {} no {:?}, causando {} de dano!",
                c_name, agent_name, target_part, damage
            ),
            impact_tags: vec!["violencia".to_string(), "fauna".to_string()],
        });

        if agent_died {
            self.mark_agent_dead(
                target_agent_id,
                &format!("derrotado pela criatura {}", c_name),
            )?;
            self.end_combats_involving(target_agent_id, CombatOutcome::Death);
        } else {
            self.ensure_combat(target_agent_id, creature_id)?;
        }

        Ok(())
    }

    fn end_combats_involving(&mut self, id: u64, outcome: CombatOutcome) {
        for combat in self.combats.iter_mut() {
            if combat.status == CombatStatus::Active && combat.participants.contains(&id) {
                combat.status = CombatStatus::Ended;
                combat.outcome = outcome.clone();
                combat.end_reason = Some(format!("Participante {} foi derrotado/morto", id));
            }
        }
    }

    pub fn resolve_hunting_quests_for_creature(
        &mut self,
        caçador_id: u64,
        creature_id: u64,
    ) -> Result<()> {
        let mut completed_quests = Vec::new();
        for quest in &self.hunting_quests {
            if quest.target_creature_id == creature_id {
                completed_quests.push(quest.clone());
            }
        }

        for quest in completed_quests {
            let mut paid = false;
            if let Some(polity_id) = quest.funding_polity_id {
                if let Some(polity) = self.polities.iter_mut().find(|p| p.id == polity_id) {
                    if polity.treasury >= quest.reward_gold {
                        polity.treasury -= quest.reward_gold;
                        paid = true;
                    }
                }
            } else if let Some(hh_id) = quest.funding_household_id {
                if let Some(hh) = self.households.iter_mut().find(|h| h.id == hh_id) {
                    if hh.treasury >= quest.reward_gold {
                        hh.treasury -= quest.reward_gold;
                        paid = true;
                    }
                }
            } else {
                // Default paid by village treasury
                if self.village_economy.public_treasury >= quest.reward_gold {
                    self.village_economy.public_treasury -= quest.reward_gold;
                    paid = true;
                }
            }

            if paid {
                // Give reward to caçador
                let ent = self.find_agent_entity(caçador_id)?;
                if let Some(mut inv) = self.world.entity_mut(ent).get_mut::<InventoryComponent>() {
                    if let Some(stack) = inv
                        .0
                        .iter_mut()
                        .find(|s| s.resource_id == ResourceKind::Moedas.id())
                    {
                        stack.amount += quest.reward_gold;
                    } else {
                        inv.0.push(ResourceStack {
                            resource_id: ResourceKind::Moedas.id().to_string(),
                            amount: quest.reward_gold,
                        });
                    }
                }
            }

            // Remove quest
            self.hunting_quests.retain(|q| q.id != quest.id);
        }
        Ok(())
    }

    pub fn give_drop_to_agent(&mut self, agent_id: u64, resource_id: &str) -> Result<()> {
        let ent = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(ent);
        let mut inv = entity_mut
            .get_mut::<InventoryComponent>()
            .ok_or_else(|| anyhow!("Agent {} does not have an InventoryComponent", agent_id))?;
        Self::push_resource(&mut inv.0, resource_id, 1);
        Ok(())
    }

    fn create_legendary_story(&mut self, agent_id: u64, creature_name: &str, species: &str) {
        let id = self.next_cultural_story_id;
        self.next_cultural_story_id += 1;
        self.cultural_stories
            .push(crate::world_model::CulturalStory {
                id,
                title: format!("A Lenda do Matador de {}", creature_name),
                narrative_core: format!(
                    "Como o bravo caçador derrotou a fera lendaria {} (um {}).",
                    creature_name, species
                ),
                origin_kind: crate::world_model::CulturalStoryKind::Heroismo,
                theme: "Heroismo".to_string(),
                moral: "A coragem supera o medo".to_string(),
                cited_agent_ids: vec![agent_id],
                associated_building_id: None,
                associated_territory_id: None,
                source_event_summaries: vec![format!("Vitoria sobre {}", creature_name)],
                origin_generation: 1,
                cultural_strength: 30,
                stability: 80,
                distortion: 0,
                status: crate::world_model::StoryStatus::Emergente,
                created_day: self.day,
                last_told_tick: self.total_ticks,
                tell_count: 1,
            });
    }

    // Creature IA behavior loop, called every tick
    pub fn tick_fauna_behavior(&mut self) -> Result<()> {
        let creatures_data: Vec<(u64, String, TileCoord, u64)> = {
            let mut list = Vec::new();
            let mut query = self
                .world
                .query::<(&CreatureCore, &PositionComponent, &LifeStatusComponent)>();
            for (core, pos, life) in query.iter(&self.world) {
                if life.0 == AgentLifeStatus::Vivo {
                    list.push((
                        core.id,
                        core.species.clone(),
                        pos.0,
                        core.habitat_territory_id,
                    ));
                }
            }
            list
        };

        let human_agents: Vec<(u64, TileCoord)> = {
            let mut list = Vec::new();
            let mut query = self
                .world
                .query::<(&AgentCore, &PositionComponent, &LifeStatusComponent)>();
            for (core, pos, life) in query.iter(&self.world) {
                if life.0 == AgentLifeStatus::Vivo {
                    list.push((core.id, pos.0));
                }
            }
            list
        };

        // 1. Process active combats
        self.tick_fauna_combats()?;

        for (c_id, species, pos, habitat_tid) in creatures_data {
            let in_combat = self
                .combats
                .iter()
                .any(|c| c.status == CombatStatus::Active && c.participants.contains(&c_id));
            if in_combat {
                continue; // Combat freezes movement
            }

            // A. Species specific passive effects / logic
            match species.as_str() {
                "Silvafaro" => {
                    if self.tick_of_day + 1 == self.ticks_per_day {
                        if let Some(t) = self.territories.iter_mut().find(|t| t.id == habitat_tid) {
                            t.stability = (t.stability + 2).min(100);
                        }
                    }
                }
                "Brumalisco" => {
                    if self.tick_of_day + 1 == self.ticks_per_day {
                        if let Some(t) = self.territories.iter_mut().find(|t| t.id == habitat_tid) {
                            t.stability = (t.stability - 2).max(0);
                        }
                    }
                }
                "Pedrapiro" => {
                    let mut target_agent = None;
                    for &(a_id, a_pos) in &human_agents {
                        if pos.manhattan(a_pos) <= 1 {
                            target_agent = Some(a_id);
                            break;
                        }
                    }
                    if let Some(a_id) = target_agent {
                        self.ensure_combat(c_id, a_id)?;
                        self.apply_creature_attack_on_agent(c_id, a_id)?;
                        continue;
                    }
                }
                "Lebre-zelo" => {
                    let mut nearest_agent_pos = None;
                    for &(_, a_pos) in &human_agents {
                        if pos.manhattan(a_pos) <= 3 {
                            nearest_agent_pos = Some(a_pos);
                            break;
                        }
                    }
                    if let Some(a_pos) = nearest_agent_pos {
                        let neighbors = pos.neighbors4();
                        let mut best_neighbor = pos;
                        let mut max_dist = pos.manhattan(a_pos);
                        for n in neighbors {
                            if self.is_walkable(n) {
                                let d = n.manhattan(a_pos);
                                if d > max_dist {
                                    max_dist = d;
                                    best_neighbor = n;
                                }
                            }
                        }
                        if best_neighbor != pos {
                            let ent = self.find_creature_entity(c_id)?;
                            self.world
                                .entity_mut(ent)
                                .get_mut::<PositionComponent>()
                                .unwrap()
                                .0 = best_neighbor;
                        }
                        continue;
                    }
                }
                _ => {}
            }

            // B. Habitat return routine
            let in_habitat = self
                .territories
                .iter()
                .find(|t| t.id == habitat_tid)
                .map(|t| t.tile_coords.contains(&pos))
                .unwrap_or(false);

            if !in_habitat {
                let ent = self.find_creature_entity(c_id)?;
                let mut c_mut = self.world.entity_mut(ent);
                let mut path_comp = c_mut.get_mut::<PathComponent>().unwrap();

                if path_comp.0.is_empty() {
                    if let Some(t) = self.territories.iter().find(|t| t.id == habitat_tid) {
                        if let Some(&goal) = t.tile_coords.first() {
                            drop(path_comp);
                            drop(c_mut);
                            if let Some(new_path) = self.find_path(pos, goal, None) {
                                let ent2 = self.find_creature_entity(c_id)?;
                                self.world
                                    .entity_mut(ent2)
                                    .get_mut::<PathComponent>()
                                    .unwrap()
                                    .0 = new_path;
                            }
                        }
                    }
                } else {
                    let next_pos = path_comp.0.remove(0);
                    c_mut.get_mut::<PositionComponent>().unwrap().0 = next_pos;
                }
            }
        }

        Ok(())
    }

    // Ticks active combats involving creatures
    pub fn tick_fauna_combats(&mut self) -> Result<()> {
        let active_combats: Vec<CombatState> = self
            .combats
            .iter()
            .filter(|c| c.status == CombatStatus::Active)
            .cloned()
            .collect();

        for combat in active_combats {
            let p1 = combat.participants[0];
            let p2 = combat.participants[1];

            let c1_is_creature = self.is_creature(p1);
            let c2_is_creature = self.is_creature(p2);

            if c1_is_creature && !c2_is_creature {
                if self.is_creature_alive(p1)? && self.life_status(p2)? == AgentLifeStatus::Vivo {
                    self.apply_creature_attack_on_agent(p1, p2)?;
                }
            } else if !c1_is_creature && c2_is_creature {
                if self.life_status(p1)? == AgentLifeStatus::Vivo && self.is_creature_alive(p2)? {
                    self.apply_creature_attack_on_agent(p2, p1)?;
                }
            }
        }

        Ok(())
    }

    // Generates hunting quests daily for aggressive creatures near the village or controlled territories
    pub fn generate_fauna_quests(&mut self) -> Result<()> {
        let aggressive_creatures: Vec<(u64, String, TileCoord, u64)> = {
            let mut list = Vec::new();
            let mut query = self
                .world
                .query::<(&CreatureCore, &PositionComponent, &LifeStatusComponent)>();
            for (core, pos, life) in query.iter(&self.world) {
                if life.0 == AgentLifeStatus::Vivo
                    && (core.species == "Pedrapiro" || core.species == "Brumalisco")
                {
                    list.push((core.id, core.name.clone(), pos.0, core.habitat_territory_id));
                }
            }
            list
        };

        for (c_id, c_name, pos, habitat_tid) in aggressive_creatures {
            if self
                .hunting_quests
                .iter()
                .any(|q| q.target_creature_id == c_id)
            {
                continue;
            }

            let mut funding_polity_id = None;
            if let Some(t) = self.territories.iter().find(|t| t.id == habitat_tid) {
                if t.controller_polity_id != 999 && t.controller_polity_id != 0 {
                    funding_polity_id = Some(t.controller_polity_id);
                }
            }

            let quest_id = self.next_hunting_quest_id;
            self.next_hunting_quest_id += 1;

            let reward_gold = if funding_polity_id.is_some() { 100 } else { 50 };

            self.hunting_quests.push(HuntingQuest {
                id: quest_id,
                target_creature_id: c_id,
                reward_gold,
                funding_polity_id,
                funding_household_id: None,
            });

            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: 0,
                target: Some(c_id),
                kind: EventKind::CreatureQuestCreated,
                summary: format!(
                    "Nova missao de caça criada para a criatura {}! Recompensa: {} moedas.",
                    c_name, reward_gold
                ),
                impact_tags: vec!["caça".to_string(), "quest".to_string()],
            });
        }

        Ok(())
    }
}
