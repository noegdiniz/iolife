use super::*;
use crate::world_model::{BodyPartKind, PartInjuryStatus};
// Violence, theft, crime case and justice systems.

impl Simulation {
    pub(super) fn apply_assault_intent(
        &mut self,
        actor_id: u64,
        target_id: Option<u64>,
    ) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        self.apply_attack(actor_id, target_id, false)
    }

    pub(super) fn apply_combat_intent(
        &mut self,
        actor_id: u64,
        target_id: Option<u64>,
    ) -> Result<()> {
        let Some(target_id) = target_id else {
            return Ok(());
        };
        self.apply_attack(actor_id, target_id, true)
    }

    pub fn apply_attack(
        &mut self,
        actor_id: u64,
        target_id: u64,
        continuing_combat: bool,
    ) -> Result<()> {
        if self.is_creature(target_id) {
            let actor_pos = self.agent_position(actor_id)?;
            let creature_pos = self.creature_position(target_id)?;
            if actor_pos.manhattan(creature_pos) <= 1 {
                return self.apply_attack_on_creature(actor_id, target_id, continuing_combat);
            } else {
                let actor_name = self.agent_name(actor_id)?;
                let target_name = self.creature_name(target_id)?;
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: actor_id,
                    target: Some(target_id),
                    kind: EventKind::Blocking,
                    summary: format!(
                        "{actor_name} tenta agredir a criatura {target_name}, mas nao esta adjacente."
                    ),
                    impact_tags: vec!["violencia".to_string(), "distancia".to_string()],
                });
                return Ok(());
            }
        }

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
        let weapon_profile = self.equipped_weapon_profile(actor_id);
        let weapon_item_id = weapon_profile.as_ref().map(|(item, _)| item.id);
        let weapon_damage = weapon_profile
            .as_ref()
            .map(|(_, profile)| profile.damage)
            .unwrap_or(0);
        let weapon_precision = weapon_profile
            .as_ref()
            .map(|(_, profile)| profile.precision)
            .unwrap_or(0);
        let weapon_severity = weapon_profile
            .as_ref()
            .map(|(_, profile)| profile.injury_severity)
            .unwrap_or(0);
        let armor_protection = self.total_armor_protection(target_id);
        let body_armor_item_id = self
            .equipped_item_in_slot(target_id, EquipmentSlot::Body)
            .map(|item| item.id);
        let shield_item_id = self
            .equipped_item_in_slot(target_id, EquipmentSlot::OffHand)
            .map(|item| item.id);
        let base_damage = if continuing_combat { 7 } else { 10 };
        let energy_bonus = if actor_state.energy >= 50 { 3 } else { 0 };
        let vulnerability_bonus = if target_life_before == AgentLifeStatus::Incapacitado {
            8
        } else {
            0
        };
        let damage = (base_damage + weapon_damage + energy_bonus + vulnerability_bonus
            - armor_protection)
            .max(1);
        let severity_bias = weapon_severity + weapon_precision / 2;
        let mut target_died = false;
        let mut target_incapacitated = false;

        let actor_name = self.agent_name(actor_id)?;
        let target_name = self.agent_name(target_id)?;
        let weapon_name = weapon_profile
            .as_ref()
            .map(|(item, _)| item.display_name.clone())
            .unwrap_or_else(|| "mãos nuas".to_string());
        drop(weapon_profile);
        let mut visceral_desc = String::new();

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

            // --- Seleção de Parte do Corpo Alvo ---
            let hash = (actor_id ^ target_id ^ self.total_ticks) as usize;
            let roll = hash % 100;
            let (target_part_kind, part_label) = if roll < 30 {
                (BodyPartKind::Chest, "Peito")
            } else if roll < 55 {
                if roll % 2 == 0 {
                    (BodyPartKind::LeftArm, "Braço Esquerdo")
                } else {
                    (BodyPartKind::RightArm, "Braço Direito")
                }
            } else if roll < 80 {
                if roll % 2 == 0 {
                    (BodyPartKind::LeftLeg, "Perna Esquerda")
                } else {
                    (BodyPartKind::RightLeg, "Perna Direita")
                }
            } else if roll < 90 {
                (BodyPartKind::Abdomen, "Abdômen")
            } else if roll < 95 {
                (BodyPartKind::Neck, "Pescoço")
            } else if roll < 97 {
                if roll % 2 == 0 {
                    (BodyPartKind::LeftEye, "Olho Esquerdo")
                } else {
                    (BodyPartKind::RightEye, "Olho Direito")
                }
            } else if roll < 99 {
                (BodyPartKind::Head, "Cabeça")
            } else {
                (BodyPartKind::Heart, "Coração")
            };

            if let Some(part) = injury
                .0
                .body_parts
                .iter_mut()
                .find(|p| p.kind == target_part_kind)
            {
                part.health = (part.health - damage).clamp(0, 100);
                if part.health <= 0 {
                    let sever_roll = ((hash >> 3) % 10) as i32 - severity_bias / 4;
                    if matches!(
                        target_part_kind,
                        BodyPartKind::LeftArm
                            | BodyPartKind::RightArm
                            | BodyPartKind::LeftHand
                            | BodyPartKind::RightHand
                            | BodyPartKind::LeftLeg
                            | BodyPartKind::RightLeg
                            | BodyPartKind::LeftFoot
                            | BodyPartKind::RightFoot
                            | BodyPartKind::LeftEye
                            | BodyPartKind::RightEye
                            | BodyPartKind::Neck
                    ) && sever_roll < 4
                    {
                        part.status = PartInjuryStatus::Severed;
                    } else {
                        part.status = PartInjuryStatus::Destroyed;
                    }
                    part.pain = 100;
                    part.bleeding = (6 + weapon_severity / 3).clamp(1, 10);
                } else if damage >= 13 {
                    let fracture_roll = ((hash >> 4) % 10) as i32 - severity_bias / 5;
                    if matches!(
                        target_part_kind,
                        BodyPartKind::LeftArm
                            | BodyPartKind::RightArm
                            | BodyPartKind::LeftLeg
                            | BodyPartKind::RightLeg
                            | BodyPartKind::Chest
                    ) && fracture_roll < 5
                    {
                        part.status = PartInjuryStatus::Fractured;
                        part.pain += damage * 2;
                        part.bleeding += 1;
                    } else {
                        part.status = PartInjuryStatus::Lacerated;
                        part.pain += damage / 2;
                        part.bleeding += (2 + weapon_severity / 4).clamp(1, 4);
                    }
                } else {
                    let bruise_roll = ((hash >> 5) % 10) as i32 + armor_protection / 4;
                    if bruise_roll < 7 && weapon_severity < 6 {
                        part.status = PartInjuryStatus::Bruised;
                        part.pain += damage;
                    } else {
                        part.status = PartInjuryStatus::Lacerated;
                        part.pain += damage / 2;
                        part.bleeding += 1;
                    }
                }

                // Descrição visceral correspondente
                match part.status {
                    PartInjuryStatus::Severed => {
                        if target_part_kind == BodyPartKind::Neck {
                            visceral_desc = format!(
                                "{} desferiu um golpe violento decepando o pescoço de {}, cuja cabeça rolou em meio a jorros de sangue quente.",
                                actor_name, target_name
                            );
                        } else {
                            visceral_desc = format!(
                                "{} acertou um golpe brutal que decepou o(a) {} de {}, deixando-o mutilado e urrando sob uma poça de sangue.",
                                actor_name, part_label, target_name
                            );
                        }
                    }
                    PartInjuryStatus::Destroyed => {
                        if target_part_kind == BodyPartKind::Heart
                            || target_part_kind == BodyPartKind::Chest
                        {
                            visceral_desc = format!(
                                "{} perfurou o peito de {}, dilacerando seu coração em um golpe letal que fez o sangue borbulhar por sua boca.",
                                actor_name, target_name
                            );
                        } else if target_part_kind == BodyPartKind::Head {
                            visceral_desc = format!(
                                "{} esmagou o crânio de {} com um impacto avassalador, espalhando massa encefálica e sangue pelo chão.",
                                actor_name, target_name
                            );
                        } else if target_part_kind == BodyPartKind::LeftEye
                            || target_part_kind == BodyPartKind::RightEye
                        {
                            visceral_desc = format!(
                                "{} cravou sua arma diretamente no(a) {} de {}, estourando o globo ocular sob gritos de agonia terríveis.",
                                actor_name, part_label, target_name
                            );
                        } else {
                            visceral_desc = format!(
                                "{} dilacerou o(a) {} de {} completamente, destruindo a musculatura sob dor excruciante.",
                                actor_name, part_label, target_name
                            );
                        }
                    }
                    PartInjuryStatus::Fractured => {
                        visceral_desc = format!(
                            "{} desferiu um impacto esmagador no(a) {} de {}, quebrando o osso com um estalo horrendo e audível de fratura.",
                            actor_name, part_label, target_name
                        );
                    }
                    PartInjuryStatus::Lacerated => {
                        visceral_desc = format!(
                            "{} cortou profundamente o(a) {} de {} com uma lâmina afiada, rasgando a carne com sangramento abundante.",
                            actor_name, part_label, target_name
                        );
                    }
                    PartInjuryStatus::Bruised => {
                        visceral_desc = format!(
                            "{} desferiu um soco pesado no(a) {} de {}, deixando uma contusão arroxeada e extremamente dolorida.",
                            actor_name, part_label, target_name
                        );
                    }
                    PartInjuryStatus::Intact => {
                        visceral_desc = format!(
                            "{} agrediu {} no(a) {}, mas não causou lesões visíveis.",
                            actor_name, target_name, part_label
                        );
                    }
                }

                if matches!(
                    target_part_kind,
                    BodyPartKind::Head | BodyPartKind::Heart | BodyPartKind::Neck
                ) && (part.status == PartInjuryStatus::Destroyed
                    || part.status == PartInjuryStatus::Severed)
                {
                    target_died = true;
                }
            }

            if damage >= 16 {
                injury.0.severe_wounds = injury.0.severe_wounds.saturating_add(1);
            } else {
                injury.0.light_wounds = injury.0.light_wounds.saturating_add(1);
            }

            let total_pain: i32 = injury.0.body_parts.iter().map(|p| p.pain).sum();
            let total_bleeding: i32 = injury.0.body_parts.iter().map(|p| p.bleeding).sum();
            injury.0.pain = total_pain.clamp(0, 100);
            injury.0.bleeding = total_bleeding.clamp(0, 10);
            injury.0.recovery_ticks = injury.0.recovery_ticks.max(30);
            drop(injury);

            if remaining_health <= 0 {
                target_died = true;
            }

            if target_died {
                entity_mut
                    .get_mut::<LifeStatusComponent>()
                    .ok_or_else(|| anyhow!("missing life status component"))?
                    .0 = AgentLifeStatus::Morto;
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
        if let Some(item_id) = weapon_item_id {
            let _ = self.degrade_item_instance(item_id, 1);
        }
        if let Some(item_id) = body_armor_item_id {
            let _ = self.degrade_item_instance(item_id, 1);
        }
        if let Some(item_id) = shield_item_id {
            let _ = self.degrade_item_instance(item_id, 1);
        }

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
            summary: format!("{visceral_desc} [arma={weapon_name} dano={damage} armadura_alvo={armor_protection}]"),
            impact_tags: vec![
                "violencia".to_string(),
                "crime".to_string(),
                weapon_name.clone(),
            ],
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
            format!("{} me atacou fisicamente. {}", actor_name, visceral_desc),
            vec!["violencia".to_string(), "ofensa".to_string()],
            35,
            vec![actor_id],
        )?;
        self.add_memory(
            actor_id,
            MemoryKind::Offense,
            format!("Eu ataquei {} fisicamente. {}", target_name, visceral_desc),
            vec!["violencia".to_string(), "culpa".to_string()],
            24,
            vec![target_id],
        )?;
        {
            let mut delta = PsychologicalState::zero_delta();
            delta.guilt = if target_died { 22 } else { 10 };
            delta.anger = 5;
            self.adjust_psychological_state(actor_id, delta, "culpa por agressao")?;
        }

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

        self.mark_revenge_target(
            target_id,
            actor_id,
            if target_died { 25 } else { 16 },
            format!("agressao sofrida de {}", actor_name),
        )?;

        // Trauma traits for victim
        let event_kind = if target_died {
            EventKind::Death
        } else {
            EventKind::Violence
        };
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

    pub(super) fn apply_robbery_intent(
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
        let stolen = self.transfer_stolen_material(actor_id, target_id, true)?;
        self.apply_attack(actor_id, target_id, false)?;
        let actor_name = self.agent_name(actor_id)?;
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

        self.mark_revenge_target(
            target_id,
            actor_id,
            14,
            format!("roubo sofrido de {}", actor_name),
        )?;

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

    pub(super) fn apply_theft_intent(
        &mut self,
        actor_id: u64,
        target_id: Option<u64>,
    ) -> Result<()> {
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

        self.mark_revenge_target(
            target_id,
            actor_id,
            12,
            format!("furto sofrido de {}", actor_name),
        )?;

        // Trauma traits for theft victim
        self.apply_trauma_traits_for_event(target_id, "victim", EventKind::Theft)?;

        Ok(())
    }

    pub(super) fn apply_flee_intent(&mut self, actor_id: u64) -> Result<()> {
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

    pub(super) fn apply_accuse_intent(
        &mut self,
        actor_id: u64,
        target_id: Option<u64>,
    ) -> Result<()> {
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

    pub(super) fn apply_investigate_intent(&mut self, actor_id: u64) -> Result<()> {
        if !self.has_justice_authority(actor_id)? {
            return Ok(());
        }
        let actor_name = self.agent_name(actor_id)?;

        // Check for false rumors that the guard knows about a target agent
        let target_agent = {
            let ent = self.find_agent_entity(actor_id)?;
            let entry = self.world.entity(ent);
            entry
                .get::<IntentComponent>()
                .and_then(|i| i.0.as_ref())
                .and_then(|i| i.target_agent)
        };

        if let Some(target_id) = target_agent {
            if self.agents_adjacent(actor_id, target_id)? {
                let mut slander_secret_created = false;
                for rumor in self.rumors.clone() {
                    if rumor.target_agent_id == target_id
                        && rumor.is_slander
                        && !rumor.is_confirmed
                        && rumor.known_by.contains(&actor_id)
                    {
                        let creator_id = rumor.source_agent_id;
                        let creator_name = self.agent_name(creator_id)?;
                        let target_name = self.agent_name(target_id)?;

                        let secret_id = {
                            let id = self.next_secret_id;
                            self.next_secret_id += 1;
                            id
                        };

                        let secret = Secret {
                            id: secret_id,
                            kind: SecretKind::SlanderCalumny,
                            target_id: creator_id,
                            summary: format!(
                                "A calÃºnia criada por {} contra {}.",
                                creator_name, target_name
                            ),
                            details: format!("Espalhou o boato falso: {}", rumor.claim),
                            known_by: vec![actor_id, creator_id],
                        };
                        self.secrets.push(secret);
                        if let Some(stored_rumor) =
                            self.rumors.iter_mut().find(|entry| entry.id == rumor.id)
                        {
                            stored_rumor.is_disproven = true;
                            stored_rumor.credibility_seed =
                                (stored_rumor.credibility_seed - 30).clamp(0, 100);
                        }
                        slander_secret_created = true;

                        self.push_event(WorldEvent {
                            day: self.day,
                            tick: self.tick_of_day,
                            actor: actor_id,
                            target: Some(creator_id),
                            kind: EventKind::Investigation,
                            summary: format!(
                                "{} investigou e provou que o boato de {} sobre {} Ã© calÃºnia.",
                                actor_name, creator_name, target_name
                            ),
                            impact_tags: vec![
                                "investigacao".to_string(),
                                "justica".to_string(),
                                "calunia".to_string(),
                            ],
                        });
                        break;
                    }
                }
                if slander_secret_created {
                    return Ok(());
                }
            }
        }

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
        let mut guard_delta = InstitutionalPerception::zero_delta();
        guard_delta.guard_trust = 4;
        guard_delta.justice_legitimacy = 3;
        guard_delta.perceived_fairness = 2;
        self.adjust_institutional_perception(
            actor_id,
            guard_delta,
            format!("investigou caso criminal {}", case_id),
        )?;
        Ok(())
    }

    pub(super) fn apply_arrest_intent(
        &mut self,
        actor_id: u64,
        target_id: Option<u64>,
    ) -> Result<()> {
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

    pub(super) fn apply_punish_intent(
        &mut self,
        actor_id: u64,
        target_id: Option<u64>,
    ) -> Result<()> {
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
        let victim_id = case.victim_id;
        let witnesses = case.witnesses.clone();
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

        self.mark_public_humiliation(
            target_id,
            Some(actor_id),
            i32::from(severity).clamp(8, 22),
            format!("punicao publica no caso {}", case_id),
        )?;

        // Trauma traits for punished agent
        self.apply_trauma_traits_for_event(target_id, "victim", EventKind::Punishment)?;
        let mut suspect_delta = InstitutionalPerception::zero_delta();
        suspect_delta.justice_legitimacy = -(i32::from(severity) / 8).clamp(4, 16);
        suspect_delta.guard_trust = -4;
        suspect_delta.fear_of_authority = (i32::from(severity) / 10).clamp(2, 12);
        if justice_severity == JusticeSeverity::Severe {
            suspect_delta.perceived_corruption += 4;
            suspect_delta.perceived_fairness -= 8;
        }
        self.adjust_institutional_perception(
            target_id,
            suspect_delta,
            format!("punido no caso {}", case_id),
        )?;
        let mut observer_delta = InstitutionalPerception::zero_delta();
        observer_delta.justice_legitimacy = (i32::from(severity) / 10).clamp(2, 10);
        observer_delta.guard_trust = 3;
        observer_delta.perceived_fairness = 4;
        if justice_severity == JusticeSeverity::Severe && severity < 50 {
            observer_delta.justice_legitimacy -= 10;
            observer_delta.perceived_corruption += 5;
            observer_delta.perceived_fairness -= 8;
        }
        let mut observers = witnesses;
        if let Some(victim_id) = victim_id {
            observers.push(victim_id);
        }
        observers.sort_unstable();
        observers.dedup();
        for observer_id in observers {
            if observer_id != target_id {
                self.adjust_institutional_perception(
                    observer_id,
                    observer_delta.clone(),
                    format!("observou punicao no caso {}", case_id),
                )?;
            }
        }

        Ok(())
    }

    pub(super) fn can_receive_violence(&mut self, agent_id: u64) -> Result<bool> {
        let status = self.life_status(agent_id)?;
        Ok(status != AgentLifeStatus::Morto)
    }

    pub(super) fn life_status(&mut self, agent_id: u64) -> Result<AgentLifeStatus> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<LifeStatusComponent>()
            .ok_or_else(|| anyhow!("missing life status component"))?
            .0)
    }

    pub(super) fn interrupt_agent_conversations(
        &mut self,
        agent_id: u64,
        outcome: ConversationOutcome,
    ) -> Result<()> {
        let conversation_ids = self
            .conversations
            .iter()
            .filter(|conversation| {
                conversation.status == ConversationStatus::Active
                    && conversation.participant_ids.contains(&agent_id)
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

    pub(super) fn mark_agent_dead(&mut self, agent_id: u64, reason: &str) -> Result<()> {
        if self.life_status(agent_id)? == AgentLifeStatus::Morto {
            return Ok(());
        }

        // 1. Gather deceased details
        let mut deceased_parents = Vec::new();
        let mut deceased_spouse = None;
        let mut deceased_name = String::new();
        let mut deceased_role_id = String::new();
        let mut deceased_work_building_id = None;
        let mut deceased_home_building_id = None;

        if let Ok(deceased_entity) = self.find_agent_entity(agent_id) {
            let deceased_entry = self.world.entity(deceased_entity);
            if let Some(core) = deceased_entry.get::<AgentCore>() {
                deceased_name = core.name.clone();
                deceased_role_id = core.role_id.clone();
                deceased_work_building_id = core.work_building_id;
                deceased_home_building_id = core.home_building_id;
            }
            if let Some(lin) = deceased_entry.get::<LineageComponent>() {
                deceased_parents = lin.parents.clone();
                deceased_spouse = lin.spouse;
            }
        }

        // 2. Find heirs (living spouse and children)
        let mut heirs = Vec::new();
        if let Some(sp) = deceased_spouse {
            if let Ok(sp_entity) = self.find_agent_entity(sp) {
                if self
                    .world
                    .entity(sp_entity)
                    .get::<LifeStatusComponent>()
                    .unwrap()
                    .0
                    == AgentLifeStatus::Vivo
                {
                    heirs.push(sp);
                }
            }
        }
        let children = if let Ok(deceased_entity) = self.find_agent_entity(agent_id) {
            self.world
                .entity(deceased_entity)
                .get::<LineageComponent>()
                .map(|l| l.children.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        for child_id in &children {
            if let Ok(ch_entity) = self.find_agent_entity(*child_id) {
                if self
                    .world
                    .entity(ch_entity)
                    .get::<LifeStatusComponent>()
                    .unwrap()
                    .0
                    == AgentLifeStatus::Vivo
                {
                    heirs.push(*child_id);
                }
            }
        }

        // 3. Immediately assign job to oldest unemployed adult child if possible
        if deceased_role_id != "campones" && deceased_role_id != "crianca" {
            let mut best_child = None;
            let mut max_age = 0;
            for child_id in &children {
                if let Ok(ch_entity) = self.find_agent_entity(*child_id) {
                    let ch_entry = self.world.entity(ch_entity);
                    if ch_entry.get::<LifeStatusComponent>().unwrap().0 == AgentLifeStatus::Vivo {
                        let ch_lin = ch_entry.get::<LineageComponent>().unwrap();
                        let ch_core = ch_entry.get::<AgentCore>().unwrap();
                        if ch_lin.age >= 18 && ch_core.role_id == "campones" {
                            if ch_lin.age > max_age {
                                max_age = ch_lin.age;
                                best_child = Some(*child_id);
                            }
                        }
                    }
                }
            }
            if let Some(child_id) = best_child {
                if let Ok(ch_entity) = self.find_agent_entity(child_id) {
                    let mut ch_entry_mut = self.world.entity_mut(ch_entity);
                    if let Some(mut ch_core) = ch_entry_mut.get_mut::<AgentCore>() {
                        ch_core.role_id = deceased_role_id.clone();
                        ch_core.work_building_id = deceased_work_building_id;
                    }
                    self.add_memory(
                        child_id,
                        MemoryKind::Success,
                        format!("Herdei o papel de meu pai/mÃ£e como {}.", deceased_role_id),
                        vec!["heranca".to_string(), "papel".to_string()],
                        15,
                        Vec::new(),
                    )?;
                }
            }
        }

        // 4. Transfer money (coins)
        let mut cash_to_transfer = 0;
        if let Ok(deceased_entity) = self.find_agent_entity(agent_id) {
            let mut deceased_entry_mut = self.world.entity_mut(deceased_entity);
            if let Some(mut inv) = deceased_entry_mut.get_mut::<InventoryComponent>() {
                if let Some(money_stack) = inv
                    .0
                    .iter_mut()
                    .find(|stack| stack.resource_id == ResourceKind::Moedas.id())
                {
                    cash_to_transfer = money_stack.amount;
                    money_stack.amount = 0;
                }
            }
        }

        if cash_to_transfer > 0 && !heirs.is_empty() {
            let share = cash_to_transfer / heirs.len() as i32;
            let remainder = cash_to_transfer % heirs.len() as i32;
            for (idx, &heir_id) in heirs.iter().enumerate() {
                let amount = share + if idx == 0 { remainder } else { 0 };
                if amount > 0 {
                    if let Ok(heir_entity) = self.find_agent_entity(heir_id) {
                        let mut heir_entry_mut = self.world.entity_mut(heir_entity);
                        if let Some(mut inv) = heir_entry_mut.get_mut::<InventoryComponent>() {
                            if let Some(money_stack) = inv
                                .0
                                .iter_mut()
                                .find(|stack| stack.resource_id == ResourceKind::Moedas.id())
                            {
                                money_stack.amount += amount;
                            } else {
                                inv.0.push(ResourceStack {
                                    resource_id: ResourceKind::Moedas.id().to_string(),
                                    amount,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 5. Gather all grieving parties (surviving spouse, children, and parents)
        let mut grieving_parties = heirs.clone();
        for &parent_id in &deceased_parents {
            if let Ok(p_entity) = self.find_agent_entity(parent_id) {
                if self
                    .world
                    .entity(p_entity)
                    .get::<LifeStatusComponent>()
                    .unwrap()
                    .0
                    == AgentLifeStatus::Vivo
                {
                    if !grieving_parties.contains(&parent_id) {
                        grieving_parties.push(parent_id);
                    }
                }
            }
        }

        // Apply grief to family members
        for &family_id in &grieving_parties {
            if let Ok(family_entity) = self.find_agent_entity(family_id) {
                let mut family_entry_mut = self.world.entity_mut(family_entity);
                if let Some(mut state) = family_entry_mut.get_mut::<StateComponent>() {
                    state.0.stress = 100;
                    state.0.mood = (state.0.mood - 40).clamp(0, 100);
                }
                if let Some(mut lineage) = family_entry_mut.get_mut::<LineageComponent>() {
                    lineage.mourning_days_left = 3;
                }
                if let Some(mut profile) = family_entry_mut.get_mut::<ProfileComponent>() {
                    if !profile.0.traits.contains(&"luto".to_string()) {
                        profile.0.traits.push("luto".to_string());
                    }
                }
                if let Some(mut psychology) =
                    family_entry_mut.get_mut::<PsychologicalStateComponent>()
                {
                    let mut delta = PsychologicalState::zero_delta();
                    delta.grief = 35;
                    delta.trauma = 10;
                    delta.fear = 5;
                    psychology
                        .0
                        .add_delta(&delta, self.day, "luto familiar".to_string());
                }
                let relation_type = if Some(family_id) == deceased_spouse {
                    "cÃ´njuge"
                } else if deceased_parents.contains(&family_id) {
                    "filho/filha"
                } else {
                    "pai/mÃ£e"
                };
                self.add_memory(
                    family_id,
                    MemoryKind::Offense,
                    format!(
                        "Estou de luto pela morte de meu {} {}.",
                        relation_type, deceased_name
                    ),
                    vec!["luto".to_string(), "perda".to_string()],
                    40,
                    vec![agent_id],
                )?;
            }
        }

        // 6. Clear deceased's spouse link on the surviving spouse
        if let Some(sp) = deceased_spouse {
            if let Ok(sp_entity) = self.find_agent_entity(sp) {
                let mut sp_entry_mut = self.world.entity_mut(sp_entity);
                if let Some(mut sp_lin) = sp_entry_mut.get_mut::<LineageComponent>() {
                    sp_lin.spouse = None;
                }
            }
        }

        // 7. Remove deceased from household
        if let Some(home_id) = deceased_home_building_id {
            if let Some(household) = self.households.iter_mut().find(|h| h.id == home_id) {
                household.member_ids.retain(|&id| id != agent_id);
            }
        }

        // 8. Update Bevy core structures
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
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::Death,
            summary: format!("{deceased_name} morre: {reason}."),
            impact_tags: vec!["morte".to_string(), "violencia".to_string()],
        });
        Ok(())
    }

    pub(super) fn ensure_combat(&mut self, actor_id: u64, target_id: u64) -> Result<()> {
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

    pub(super) fn open_crime_case_if_observed(
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
        if let Some(suspect) = suspect_id {
            self.generate_crime_secret(id, suspect, victim_id, &witnesses, victim_conscious)?;
        }

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

    pub(super) fn generate_crime_secret(
        &mut self,
        case_id: CrimeCaseId,
        suspect_id: u64,
        victim_id: Option<u64>,
        witnesses: &[u64],
        victim_conscious: bool,
    ) -> Result<()> {
        let secret_id = self.next_secret_id;
        self.next_secret_id += 1;

        let victim_name = match victim_id {
            Some(vid) => self.agent_name(vid)?,
            None => "alguÃ©m".to_string(),
        };

        let mut known_by = vec![suspect_id];
        known_by.extend(witnesses.iter().copied());
        if victim_conscious {
            if let Some(vid) = victim_id {
                known_by.push(vid);
            }
        }
        known_by.sort();
        known_by.dedup();

        let secret = Secret {
            id: secret_id,
            kind: SecretKind::CrimeCulprit,
            target_id: case_id,
            summary: format!("A autoria do crime contra {}.", victim_name),
            details: suspect_id.to_string(),
            known_by,
        };

        self.secrets.push(secret);
        Ok(())
    }

    pub(super) fn transfer_stolen_material(
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
            let stolen_item_id = {
                let mut target_mut = self.world.entity_mut(target_entity);
                target_mut
                    .get_mut::<ItemInventoryComponent>()
                    .and_then(|mut items| {
                        if items.0.is_empty() {
                            None
                        } else {
                            Some(items.0.remove(0))
                        }
                    })
            };
            if let Some(item_id) = stolen_item_id {
                let item_name = self.item_display_name_for_id(item_id);
                let actor_household_id = self.household_id_for_agent(actor_id);
                {
                    let mut actor_mut = self.world.entity_mut(actor_entity);
                    actor_mut
                        .get_mut::<ItemInventoryComponent>()
                        .ok_or_else(|| anyhow!("missing item inventory component"))?
                        .0
                        .push(item_id);
                }
                if let Some(item) = self.item_instance_mut(item_id) {
                    item.owner_agent_id = Some(actor_id);
                    item.owner_household_id = actor_household_id;
                }
                let _ = self.maybe_auto_equip_best_items(actor_id);
                stolen.push(item_name);
                return Ok(stolen);
            }
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

    pub(super) fn witnesses_near(
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

    pub(super) fn injury_summary_for_agent(&mut self, agent_id: u64) -> String {
        let mut query = self.world.query::<(&AgentCore, &InjuryComponent)>();
        query
            .iter(&self.world)
            .find_map(|(core, injury)| {
                (core.id == agent_id).then(|| {
                    let mut parts_desc = Vec::new();
                    for part in &injury.0.body_parts {
                        if part.status != PartInjuryStatus::Intact {
                            parts_desc.push(format!(
                                "{}:{:?}({}%)",
                                part.kind.display_name(),
                                part.status,
                                part.health
                            ));
                        }
                    }
                    let parts_str = if parts_desc.is_empty() {
                        "Intacto".to_string()
                    } else {
                        parts_desc.join(",")
                    };
                    format!(
                        "leves={} graves={} dor={} sangramento={} partes=[{}]",
                        injury.0.light_wounds,
                        injury.0.severe_wounds,
                        injury.0.pain,
                        injury.0.bleeding,
                        parts_str
                    )
                })
            })
            .unwrap_or_else(|| "sem dados de ferimento".to_string())
    }

    pub(super) fn has_justice_authority(&mut self, agent_id: u64) -> Result<bool> {
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
}

impl Simulation {}

impl Simulation {
    pub(super) fn adjust_psychological_state(
        &mut self,
        agent_id: u64,
        delta: PsychologicalState,
        note: impl Into<String>,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut component = entity_mut
            .get_mut::<PsychologicalStateComponent>()
            .ok_or_else(|| anyhow!("missing psychological state component"))?;
        component.0.add_delta(&delta, self.day, note.into());
        Ok(())
    }

    pub(super) fn decay_psychological_states_daily(&mut self) -> Result<()> {
        let mut query = self.world.query::<&mut PsychologicalStateComponent>();
        for mut component in query.iter_mut(&mut self.world) {
            component.0.decay_daily();
        }
        Ok(())
    }

    pub(super) fn apply_trauma_trait(&mut self, agent_id: u64, trait_name: &str) -> Result<()> {
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

    pub(super) fn apply_trauma_traits_for_event(
        &mut self,
        agent_id: u64,
        role: &str,
        event_kind: EventKind,
    ) -> Result<()> {
        match (event_kind, role) {
            (EventKind::Violence, "victim") => {
                self.apply_trauma_trait(agent_id, "traumatizado")?;
                self.apply_trauma_trait(agent_id, "vingativo")?;
                let mut delta = PsychologicalState::zero_delta();
                delta.fear = 18;
                delta.trauma = 16;
                delta.anger = 14;
                self.adjust_psychological_state(agent_id, delta, "vitima de violencia")?;
            }
            (EventKind::Violence, "witness_first") => {
                self.apply_trauma_trait(agent_id, "assustado")?;
                let mut delta = PsychologicalState::zero_delta();
                delta.fear = 10;
                delta.trauma = 6;
                self.adjust_psychological_state(agent_id, delta, "testemunhou violencia")?;
            }
            (EventKind::Violence, "witness_repeat") => {
                self.apply_trauma_trait(agent_id, "insensibilizado")?;
                let mut delta = PsychologicalState::zero_delta();
                delta.fear = 4;
                delta.trauma = 4;
                self.adjust_psychological_state(agent_id, delta, "testemunhou violencia repetida")?;
            }
            (EventKind::Theft, "victim") => {
                self.apply_trauma_trait(agent_id, "desconfiado")?;
                self.apply_trauma_trait(agent_id, "vingativo")?;
                let mut delta = PsychologicalState::zero_delta();
                delta.humiliation = 10;
                delta.anger = 12;
                delta.fear = 5;
                self.adjust_psychological_state(agent_id, delta, "vitima de roubo/furto")?;
            }
            (EventKind::Death, "witness") => {
                self.apply_trauma_trait(agent_id, "traumatizado")?;
                self.apply_trauma_trait(agent_id, "nihilista")?;
                let mut delta = PsychologicalState::zero_delta();
                delta.grief = 18;
                delta.trauma = 18;
                delta.fear = 10;
                self.adjust_psychological_state(agent_id, delta, "testemunhou morte")?;
            }
            (EventKind::Punishment, "victim") => {
                self.apply_trauma_trait(agent_id, "ressentido")?;
                self.apply_trauma_trait(agent_id, "rebelde")?;
                let mut delta = PsychologicalState::zero_delta();
                delta.humiliation = 18;
                delta.fear = 10;
                delta.anger = 12;
                self.adjust_psychological_state(agent_id, delta, "foi punido publicamente")?;
            }
            _ => {}
        }
        match (event_kind, role) {
            (EventKind::Violence, "victim") => {
                self.add_personal_symbol(
                    agent_id,
                    PersonalSymbolTargetKind::Event,
                    None,
                    "violencia sofrida",
                    "o mundo pode ferir sem aviso",
                    "medo",
                    28,
                    None,
                )?;
                self.add_coping_pattern(
                    agent_id,
                    CopingPatternKind::Withdrawal,
                    "violencia",
                    "buscar abrigo antes de confiar no espaco publico",
                    18,
                )?;
            }
            (EventKind::Theft, "victim") => {
                self.add_personal_symbol(
                    agent_id,
                    PersonalSymbolTargetKind::Event,
                    None,
                    "roubo sofrido",
                    "posses pequenas carregam dignidade",
                    "desconfianca",
                    22,
                    None,
                )?;
                self.add_coping_pattern(
                    agent_id,
                    CopingPatternKind::Hoarding,
                    "perda material",
                    "guardar recursos e desconfiar de pedidos generosos",
                    18,
                )?;
            }
            (EventKind::Death, "witness") => {
                self.add_personal_symbol(
                    agent_id,
                    PersonalSymbolTargetKind::Event,
                    None,
                    "morte testemunhada",
                    "a vida pode acabar diante dos olhos",
                    "luto",
                    35,
                    None,
                )?;
                self.add_coping_pattern(
                    agent_id,
                    CopingPatternKind::RitualReturn,
                    "luto",
                    "voltar a um lugar silencioso para ordenar a perda",
                    24,
                )?;
            }
            (EventKind::Punishment, "victim") => {
                self.add_personal_symbol(
                    agent_id,
                    PersonalSymbolTargetKind::Event,
                    None,
                    "punicao publica",
                    "autoridade pode virar vergonha",
                    "humilhacao",
                    30,
                    None,
                )?;
                self.add_coping_pattern(
                    agent_id,
                    CopingPatternKind::Confrontation,
                    "punicao injusta",
                    "guardar a ofensa ate poder desafiar sem morrer",
                    20,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn propagate_witness_effects(
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
            let mut query = self
                .world
                .query::<(&AgentCore, &PositionComponent, &LifeStatusComponent)>();
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
                format!(
                    "Presenciou {} {} {}.",
                    aggressor_name, event_desc, victim_name
                ),
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

    pub(super) fn update_trauma_trackers(&mut self) -> Result<()> {
        // Collect agent data needed for continuous tracking
        let agent_data: Vec<(u64, i32, i32, i32)> = {
            let mut query = self
                .world
                .query::<(&AgentCore, &StateComponent, &LifeStatusComponent)>();
            query
                .iter(&self.world)
                .filter(|(_, _, life)| life.0 == AgentLifeStatus::Vivo)
                .map(|(core, state, _)| (core.id, state.0.hunger, state.0.stress, 0i32))
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
