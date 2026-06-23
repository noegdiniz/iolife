use super::*;
// Read-only projections used by TUI, headless mode and persistence snapshots.

use crate::world_model::{CombatStatus, PoliticalIssueStatus, SpatialSnapshot, WorldEvent};

impl Simulation {
    pub fn summary(&self) -> String {
        format!(
            "{} | Dia {} {} | Tick {} | Total {}",
            self.village_name,
            self.day,
            self.time_context().time_label,
            self.tick_of_day,
            self.total_ticks
        )
    }

    pub fn village_name(&self) -> &str {
        &self.village_name
    }

    pub fn current_day(&self) -> u32 {
        self.day
    }

    pub fn tick_of_day(&self) -> u32 {
        self.tick_of_day
    }

    pub fn total_ticks(&self) -> u64 {
        self.total_ticks
    }

    pub fn spatial(&self) -> &SpatialSnapshot {
        &self.spatial
    }

    pub fn recent_events(&self, limit: usize) -> Vec<WorldEvent> {
        self.events.iter().rev().take(limit).cloned().collect()
    }

    pub fn history_overview(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!(
            "bootstrap_historico={} anos | fundacao={} | resumo={}",
            self.world_history_years_simulated,
            self.world_foundation_year,
            self.historical_summary
                .as_ref()
                .map(|summary| format!(
                    "lares_fundadores={} sobreviventes={} vivos={}",
                    summary.founding_households,
                    summary.surviving_households,
                    summary.living_population
                ))
                .unwrap_or_else(|| "-".to_string())
        ));
        if let Some(summary) = &self.historical_summary {
            if !summary.major_dynasties.is_empty() {
                lines.push(format!("dinastias={}", summary.major_dynasties.join(", ")));
            }
            if !summary.major_conflicts.is_empty() {
                lines.push(format!("conflitos={}", summary.major_conflicts.join(", ")));
            }
            if !summary.major_foundations.is_empty() {
                lines.push(format!(
                    "fundacoes={}",
                    summary.major_foundations.join(", ")
                ));
            }
            if summary.average_territorial_stability != 0 {
                lines.push(format!(
                    "estabilidade_territorial_media={}",
                    summary.average_territorial_stability
                ));
            }
            if !summary.recent_crises.is_empty() {
                lines.push(format!("crises={}", summary.recent_crises.join(", ")));
            }
            if !summary.active_decrees.is_empty() {
                lines.push(format!("decretos={}", summary.active_decrees.join(", ")));
            }
            if !summary.dominant_stories.is_empty() {
                lines.push(format!(
                    "historias_dominantes={}",
                    summary.dominant_stories.join(", ")
                ));
            }
            if !summary.dominant_households.is_empty() {
                lines.push(format!(
                    "casas_dominantes={}",
                    summary.dominant_households.join(", ")
                ));
            }
            if !summary.recent_wars.is_empty() {
                lines.push(format!("guerras={}", summary.recent_wars.join(", ")));
            }
        }
        lines
    }

    pub fn economy_overview(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "caixa_publico={} | imposto_diario_por_lar={}",
            self.village_economy.public_treasury, self.village_economy.daily_household_tax
        )];
        for establishment in self.establishments.iter().filter(|establishment| {
            establishment.public_service || self.recipe_for_establishment(establishment).is_some()
        }) {
            let stock = establishment
                .stock
                .iter()
                .map(|stack| {
                    format!(
                        "{}x{}",
                        self.resource_display_name(&stack.resource_id),
                        stack.amount
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "{} | caixa={} | {}",
                establishment.name, establishment.cash, stock
            ));
        }
        for project in self.construction_projects.iter().filter(|project| {
            !matches!(
                project.status,
                crate::world_model::ConstructionStatus::Completed
                    | crate::world_model::ConstructionStatus::Cancelled
            )
        }) {
            let missing = project
                .materials_required
                .iter()
                .filter_map(|required| {
                    let delivered = super::Simulation::total_resource_amount(
                        &project.materials_delivered,
                        &required.resource_id,
                    );
                    let missing = required.amount - delivered;
                    (missing > 0).then(|| {
                        format!(
                            "{}x{}",
                            self.resource_display_name(&required.resource_id),
                            missing
                        )
                    })
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "obra #{} {} | status={:?} | faltam=[{}] | trabalho={}/{} | motivo={}",
                project.id,
                project.building_name,
                project.status,
                missing,
                project.labor_done,
                project.labor_required,
                project.systemic_reason
            ));
        }
        for demand in self.military_demands.iter().filter(|demand| {
            matches!(
                demand.status,
                crate::world_model::MilitaryDemandStatus::Open
                    | crate::world_model::MilitaryDemandStatus::PartiallySupplied
            )
        }) {
            let missing = super::Simulation::missing_military_resources_for_demand(demand)
                .into_iter()
                .map(|stack| {
                    format!(
                        "{}x{}",
                        self.resource_display_name(&stack.resource_id),
                        stack.amount
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!(
                "demanda_militar #{} guerra={} {:?} | faltam=[{}] | caixa={}/{} | prazo_dia={} | status={:?}",
                demand.id,
                demand.war_id,
                demand.stage,
                missing,
                demand.cash_delivered,
                demand.cash_required,
                demand.deadline_day,
                demand.status
            ));
        }
        lines
    }

    pub fn legal_overview(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for case in self.crime_cases.iter().rev().take(6) {
            lines.push(format!(
                "caso #{} {:?} status={:?} suspeito={:?} vitima={:?} severidade={} confianca={} sentenca={:?}",
                case.id,
                case.crime_type,
                case.status,
                case.suspect_id,
                case.victim_id,
                case.severity,
                case.confidence,
                case.sentence
            ));
        }
        for combat in self
            .combats
            .iter()
            .filter(|combat| combat.status == CombatStatus::Active)
            .take(4)
        {
            lines.push(format!(
                "combate #{} {:?} participantes={:?} round={}",
                combat.id, combat.status, combat.participants, combat.round
            ));
        }
        lines
    }

    pub fn politics_overview(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "normas | imposto={} | justica={} | racionamento={}",
            self.village_economy.daily_household_tax,
            self.local_norms.justice_severity.as_str(),
            self.local_norms.rationing_policy.as_str()
        )];
        lines.extend(
            self.political_issues
                .iter()
                .filter(|issue| issue.status == PoliticalIssueStatus::Open)
                .take(5)
                .map(|issue| {
                    format!(
                        "pauta #{} {} -> {} | apoio={} oposicao={} | {}",
                        issue.id,
                        issue.domain.as_str(),
                        issue.proposed_value,
                        issue.support_score,
                        issue.opposition_score,
                        issue.summary
                    )
                }),
        );
        lines.extend(
            self.policy_acts
                .iter()
                .filter(|act| self.policy_act_is_active(act))
                .take(5)
                .map(|act| {
                    format!(
                        "ato #{} {:?} | legitimidade={} aplicacao={} resistencia={} | {}",
                        act.id,
                        act.authority,
                        act.legitimacy,
                        act.enforcement,
                        act.resistance,
                        act.summary
                    )
                }),
        );
        lines.extend(self.territories.iter().take(5).map(|territory| {
            format!(
                "territorio #{} {} | controlador={} estabilidade={} valor={}",
                territory.id,
                territory.name,
                territory.controller_polity_id,
                territory.stability,
                territory.strategic_value
            )
        }));
        lines.extend(self.foreign_relations.iter().take(5).map(|relation| {
            format!(
                "relacao externa #{} {}-{} | {:?} trust={} fear={}",
                relation.id,
                relation.polity_a,
                relation.polity_b,
                relation.stance,
                relation.trust,
                relation.fear
            )
        }));
        lines.extend(self.wars.iter().take(5).map(|war| {
            let supply_score = self.recent_military_supply_score_for_polity(war.attacker_polity_id);
            format!(
                "guerra #{} {}->{} | {:?}/{:?} | placar {}-{} | suprimento_atacante={} | alvo={:?}",
                war.id,
                war.attacker_polity_id,
                war.defender_polity_id,
                war.status,
                war.stage,
                war.attacker_score,
                war.defender_score,
                supply_score,
                war.target_territory_ids
            )
        }));
        lines.extend(self.insurrections.iter().take(5).map(|insurrection| {
            format!(
                "insurreicao #{} | {:?}/{:?} | apoio={} repressao={} | faccoes={:?} | guerra={:?}",
                insurrection.id,
                insurrection.status,
                insurrection.stage,
                insurrection.popular_support,
                insurrection.repression,
                insurrection.faction_ids,
                insurrection.linked_war_id
            )
        }));
        lines.extend(self.political_factions.iter().take(5).map(|faction| {
            let active_str = if faction.is_action_active {
                "ATIVA"
            } else {
                "inativa"
            };
            format!(
                "faccao #{} {} | influencia={} | membros={} | rage={} | status={} | obj={:?}",
                faction.id,
                faction.name,
                faction.influence,
                faction.member_ids.len(),
                faction.rage,
                active_str,
                faction.objective
            )
        }));
        lines.extend(self.feudal_titles.iter().take(5).map(|title| {
            format!(
                "titulo #{} {} | rank={} holder={:?} legitimidade={} precedencia={}",
                title.id,
                title.name,
                title.rank.as_str(),
                title.holder_agent_id,
                title.legitimacy,
                title.precedence
            )
        }));
        lines.extend(self.feudal_contracts.iter().take(5).map(|contract| {
            format!(
                "contrato #{} {}->{} | tributo={} levy={} lealdade={} coercao={} status={:?}",
                contract.id,
                contract.suzerain_agent_id,
                contract.vassal_agent_id,
                contract.tribute_due_per_day,
                contract.levy_duty,
                contract.loyalty,
                contract.coercion,
                contract.status
            )
        }));
        lines.extend(self.succession_crises.iter().take(5).map(|crisis| {
            format!(
                "sucessao #{} titulo={} | status={:?} conflito={} gap={} herdeiro={:?}",
                crisis.id,
                crisis.title_id,
                crisis.status,
                crisis.conflict_score,
                crisis.legitimacy_gap,
                crisis.recognized_heir_id
            )
        }));
        lines
    }

    pub fn culture_overview(&self) -> Vec<String> {
        self.cultural_stories
            .iter()
            .filter(|story| !matches!(story.status, crate::world_model::StoryStatus::Esquecida))
            .take(6)
            .map(|story| {
                let versions = self
                    .story_versions
                    .iter()
                    .filter(|version| version.story_id == story.id)
                    .count();
                format!(
                    "historia #{} {} | {:?}/{:?} | forca={} estabilidade={} distorcao={} versoes={} | moral={}",
                    story.id,
                    story.title,
                    story.origin_kind,
                    story.status,
                    story.cultural_strength,
                    story.stability,
                    story.distortion,
                    versions,
                    story.moral
                )
            })
            .collect()
    }

    pub fn time_context(&self) -> TimeContextInput {
        let ticks_per_day = self.ticks_per_day.max(1);
        let minute_of_day =
            ((u64::from(self.tick_of_day) * 1_440) / u64::from(ticks_per_day)).min(1_439) as u32;
        let hour = minute_of_day / 60;
        let minute = minute_of_day % 60;
        let day_phase = match hour {
            0..=5 => "madrugada",
            6..=11 => "manha",
            12..=17 => "tarde",
            _ => "noite",
        }
        .to_string();
        TimeContextInput {
            day: self.day,
            tick_of_day: self.tick_of_day,
            hour,
            minute,
            time_label: format!("{hour:02}:{minute:02}"),
            day_phase,
            is_daylight: (6..=17).contains(&hour),
            is_work_time: (7..=17).contains(&hour),
            is_meal_time: (6..=8).contains(&hour)
                || (12..=14).contains(&hour)
                || (18..=20).contains(&hour),
            is_sleep_time: hour >= 21 || hour <= 5,
        }
    }

    pub fn world_place_inputs(&self) -> Vec<WorldPlaceInput> {
        self.canonical_world_places()
            .into_iter()
            .map(|place| WorldPlaceInput {
                place_id: place.place_id,
                display_name: place.display_name,
                kind: format!("{:?}", place.kind),
                semantic_tags: place.semantic_tags,
            })
            .collect()
    }

    pub fn canonical_world_places(&self) -> Vec<WorldPlaceRef> {
        let mut places = Vec::new();
        for building in &self.spatial.buildings {
            places.push(WorldPlaceRef {
                place_id: format!("building:{}", building.id),
                display_name: building.name.clone(),
                kind: WorldPlaceKind::Building,
                semantic_tags: vec![
                    "edificio".to_string(),
                    building.kind.as_str().to_lowercase(),
                    building.name.to_lowercase(),
                ],
                building_id: Some(building.id),
                room_id: None,
                fixture_id: None,
                territory_id: None,
            });
        }
        for room in &self.spatial.rooms {
            let building_name = self
                .building_name(room.building_id)
                .unwrap_or_else(|| format!("Predio {}", room.building_id));
            places.push(WorldPlaceRef {
                place_id: format!("room:{}", room.id),
                display_name: format!("{} / {}", building_name, room.name),
                kind: WorldPlaceKind::Room,
                semantic_tags: vec![
                    "sala".to_string(),
                    room.kind.to_lowercase(),
                    room.name.to_lowercase(),
                ],
                building_id: Some(room.building_id),
                room_id: Some(room.id),
                fixture_id: None,
                territory_id: None,
            });
        }
        for fixture in &self.spatial.fixtures {
            let mut tags = vec![
                "fixture".to_string(),
                format!("{:?}", fixture.kind).to_lowercase(),
                fixture.name.to_lowercase(),
            ];
            match fixture.kind {
                FixtureKind::Bed => tags.push("descanso".to_string()),
                FixtureKind::Workstation => tags.push("trabalho".to_string()),
                FixtureKind::Storage => tags.push("estoque".to_string()),
                FixtureKind::Table | FixtureKind::Seat => tags.push("social".to_string()),
            }
            places.push(WorldPlaceRef {
                place_id: format!("fixture:{}", fixture.id),
                display_name: fixture.name.clone(),
                kind: WorldPlaceKind::Fixture,
                semantic_tags: tags,
                building_id: fixture.building_id,
                room_id: fixture.room_id,
                fixture_id: Some(fixture.id),
                territory_id: None,
            });
        }
        for territory in &self.territories {
            places.push(WorldPlaceRef {
                place_id: format!("territory:{}", territory.id),
                display_name: territory.name.clone(),
                kind: WorldPlaceKind::Territory,
                semantic_tags: vec!["territorio".to_string(), territory.name.to_lowercase()],
                building_id: None,
                room_id: None,
                fixture_id: None,
                territory_id: Some(territory.id),
            });
        }
        places.push(WorldPlaceRef {
            place_id: "special:external_market".to_string(),
            display_name: "Mercado Externo".to_string(),
            kind: WorldPlaceKind::Special,
            semantic_tags: vec![
                "mercado_externo".to_string(),
                "comercio".to_string(),
                "fora_da_vila".to_string(),
            ],
            building_id: None,
            room_id: None,
            fixture_id: None,
            territory_id: None,
        });
        places.sort_by(|a, b| a.place_id.cmp(&b.place_id));
        places
    }

    pub(super) fn place_by_id(&self, place_id: &str) -> Option<WorldPlaceRef> {
        self.canonical_world_places()
            .into_iter()
            .find(|place| place.place_id == place_id)
    }

    pub(super) fn place_target_coord(&self, place_id: &str) -> Option<TileCoord> {
        if place_id == "special:external_market" {
            return Some(self.village_economy.external_market_coord);
        }
        let place = self.place_by_id(place_id)?;
        if let Some(fixture_id) = place.fixture_id {
            return self
                .spatial
                .fixtures
                .iter()
                .find(|fixture| fixture.id == fixture_id)
                .and_then(|fixture| self.fixture_access_tile(fixture).or(Some(fixture.coord)));
        }
        if let Some(room_id) = place.room_id {
            return self
                .spatial
                .rooms
                .iter()
                .find(|room| room.id == room_id)
                .and_then(|room| {
                    room.tiles
                        .iter()
                        .copied()
                        .find(|coord| self.is_walkable(*coord))
                        .or_else(|| {
                            room.tiles
                                .iter()
                                .find_map(|coord| self.access_tile_for_coord(*coord))
                        })
                });
        }
        if let Some(building_id) = place.building_id {
            return self
                .spatial
                .buildings
                .iter()
                .find(|building| building.id == building_id)
                .map(|building| building.entrance)
                .or_else(|| {
                    self.spatial
                        .buildings
                        .iter()
                        .find(|building| building.id == building_id)
                        .and_then(|building| {
                            building
                                .footprint
                                .iter()
                                .find_map(|coord| self.access_tile_for_coord(*coord))
                        })
                });
        }
        if let Some(territory_id) = place.territory_id {
            return self
                .territories
                .iter()
                .find(|territory| territory.id == territory_id)
                .and_then(|territory| {
                    territory
                        .tile_coords
                        .iter()
                        .copied()
                        .find(|coord| self.is_walkable(*coord))
                });
        }
        None
    }

    pub fn meetings_overview(&mut self) -> Vec<String> {
        let meetings = self
            .scheduled_meetings
            .iter()
            .filter(|meeting| {
                matches!(
                    meeting.status,
                    ScheduledMeetingStatus::Proposed
                        | ScheduledMeetingStatus::Accepted
                        | ScheduledMeetingStatus::Active
                )
            })
            .take(8)
            .cloned()
            .collect::<Vec<_>>();
        meetings
            .into_iter()
            .map(|meeting| {
                let proposer = self
                    .agent_name(meeting.proposer_id)
                    .unwrap_or_else(|_| format!("Agente {}", meeting.proposer_id));
                let invitee = meeting
                    .invitee_ids
                    .iter()
                    .map(|agent_id| {
                        self.agent_name(*agent_id)
                            .unwrap_or_else(|_| format!("Agente {}", agent_id))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let place = self
                    .place_by_id(&meeting.place_id)
                    .map(|place| place.display_name)
                    .unwrap_or_else(|| meeting.place_id.clone());
                format!(
                    "encontro #{} {:?} | {} + {} | Dia {} tick {} | {} | {}",
                    meeting.id,
                    meeting.status,
                    proposer,
                    invitee,
                    meeting.scheduled_day,
                    meeting.scheduled_tick,
                    place,
                    meeting.purpose
                )
            })
            .collect()
    }
}

impl Simulation {
    pub fn agent_views(&mut self) -> Vec<AgentView> {
        let agent_name_map = self.agent_name_map();
        let conversation_map = self.conversation_map();
        let rumor_belief_map = {
            let mut map = HashMap::new();
            let mut query = self.world.query::<(&AgentCore, &RumorBeliefComponent)>();
            for (core, beliefs) in query.iter(&self.world) {
                map.insert(core.id, beliefs.0.clone());
            }
            map
        };
        let story_belief_map = {
            let mut map = HashMap::new();
            let mut query = self.world.query::<(&AgentCore, &StoryBeliefComponent)>();
            for (core, beliefs) in query.iter(&self.world) {
                map.insert(core.id, beliefs.0.clone());
            }
            map
        };
        let psychological_state_map = {
            let mut map = HashMap::new();
            let mut query = self
                .world
                .query::<(&AgentCore, &PsychologicalStateComponent)>();
            for (core, psychology) in query.iter(&self.world) {
                map.insert(core.id, psychology.0.clone());
            }
            map
        };
        let mut views = Vec::new();
        let mut query = self.world.query::<(
            (
                &AgentCore,
                &StateComponent,
                &LifeStatusComponent,
                &InjuryComponent,
                &InstitutionalPerceptionComponent,
                &PositionComponent,
                &DestinationComponent,
                &DestinationLabelComponent,
            ),
            (
                &PathComponent,
                &IntentComponent,
                &ThoughtComponent,
                &MemoryComponent,
                &RelationComponent,
                &ConversationComponent,
                &EconomicActivityComponent,
                &UtilityControlComponent,
            ),
        )>();
        for (
            (
                core,
                state,
                life_status,
                injury,
                institutional_perception,
                position,
                destination,
                destination_label,
            ),
            (path, intent, thought, memories, relations, conversation, economic, utility),
        ) in query.iter(&self.world)
        {
            let psychological_state = psychological_state_map
                .get(&core.id)
                .cloned()
                .unwrap_or_default();
            let tile = self.tile_at(position.0);
            let building = tile
                .and_then(|entry| entry.building_id)
                .and_then(|id| self.building_name(id));
            let room = tile
                .and_then(|entry| entry.room_id)
                .and_then(|id| self.room_name(id));
            let household = core
                .home_building_id
                .and_then(|building_id| self.household_by_id(building_id));
            let pending_salary = household
                .map(|entry| {
                    entry
                        .pending_payments
                        .iter()
                        .map(|claim| claim.amount)
                        .sum()
                })
                .unwrap_or(0);
            let work_establishment = core
                .work_building_id
                .and_then(|building_id| self.establishment_by_building(building_id));
            let reactive_summary: crate::sim_core::utility_ai::ReactivePsychologySummary =
                Default::default();
            let scheduled_meetings = self
                .scheduled_meetings
                .iter()
                .filter(|meeting| {
                    (meeting.proposer_id == core.id || meeting.invitee_ids.contains(&core.id))
                        && matches!(
                            meeting.status,
                            ScheduledMeetingStatus::Proposed
                                | ScheduledMeetingStatus::Accepted
                                | ScheduledMeetingStatus::Active
                        )
                })
                .take(4)
                .map(|meeting| {
                    let place = self
                        .place_by_id(&meeting.place_id)
                        .map(|place| place.display_name)
                        .unwrap_or_else(|| meeting.place_id.clone());
                    format!(
                        "#{} {:?} Dia {} tick {} em {}: {}",
                        meeting.id,
                        meeting.status,
                        meeting.scheduled_day,
                        meeting.scheduled_tick,
                        place,
                        meeting.purpose
                    )
                })
                .collect();
            views.push(AgentView {
                id: core.id,
                name: core.name.clone(),
                role_id: core.role_id.clone(),
                role_name: self.role_display_name(&core.role_id),
                household_id: core.home_building_id,
                household_name: household.map(|entry| entry.name.clone()),
                area: self.area_name(position.0),
                building,
                room,
                position: position.0,
                destination: destination.0,
                destination_label: destination_label.0.clone(),
                path_len: path.0.len(),
                state: state.0.clone(),
                life_status: life_status.0,
                injury: injury.0.clone(),
                institutional_perception: institutional_perception.0.clone(),
                psychological_state,
                craft_proficiencies: self.craft_proficiencies_for_agent(core.id),
                perceived_status_score: self.perceived_status_score(core.id),
                visible_prestige_summary: self.visible_prestige_summary(core.id),
                equipped_items: self.equipped_item_summaries(core.id),
                inventory_items: self.inventory_item_summaries(core.id, 8),
                rumor_beliefs: rumor_belief_map.get(&core.id).cloned().unwrap_or_default(),
                known_rumors: rumor_belief_map
                    .get(&core.id)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|belief| {
                        self.rumors
                            .iter()
                            .find(|rumor| rumor.id == belief.rumor_id)
                            .map(|rumor| {
                                format!(
                                    "#{} {} crenca={} distorcao={}",
                                    rumor.id, rumor.claim, belief.belief, rumor.distortion
                                )
                            })
                    })
                    .take(4)
                    .collect(),
                known_stories: story_belief_map
                    .get(&core.id)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|belief| {
                        self.cultural_stories
                            .iter()
                            .find(|story| story.id == belief.story_id)
                            .map(|story| {
                                format!(
                                    "#{} {} crenca={} apego={} status={:?}",
                                    story.id,
                                    story.title,
                                    belief.belief,
                                    belief.emotional_attachment,
                                    story.status
                                )
                            })
                    })
                    .take(4)
                    .collect(),
                last_intent: intent.0.clone(),
                last_thought: thought.0.clone(),
                recent_memories: memories.0.iter().rev().take(4).cloned().collect(),
                relations: relations
                    .0
                    .iter()
                    .map(|(id, relation)| (*id, relation.clone()))
                    .collect(),
                active_conversation_id: conversation.active_conversation_id,
                conversation_participant_names: conversation
                    .conversation_participant_ids
                    .iter()
                    .filter_map(|partner_id| agent_name_map.get(partner_id).cloned())
                    .collect(),
                conversation_turn_count: conversation.active_conversation_id.and_then(
                    |conversation_id| {
                        conversation_map
                            .get(&conversation_id)
                            .map(|conversation| conversation.turn_count)
                    },
                ),
                conversation_summary: conversation.active_conversation_id.and_then(
                    |conversation_id| {
                        conversation_map
                            .get(&conversation_id)
                            .map(|conversation| conversation.summary.clone())
                    },
                ),
                speaking_now: conversation
                    .active_conversation_id
                    .and_then(|conversation_id| {
                        conversation_map
                            .get(&conversation_id)
                            .map(|conversation| conversation.current_speaker_id == core.id)
                    })
                    .unwrap_or(false),
                last_social_act: conversation.last_social_act.clone(),
                household_treasury: household.map(|entry| entry.treasury).unwrap_or(0),
                household_tax_arrears: household.map(|entry| entry.tax_arrears).unwrap_or(0),
                household_pantry: household
                    .map(|entry| entry.pantry.clone())
                    .unwrap_or_default(),
                pending_salary,
                active_task_summary: economic
                    .active_task_id
                    .and_then(|task_id| self.economic_task_summary(task_id)),
                carrying: economic.carrying.clone(),
                work_establishment_name: work_establishment.map(|entry| entry.name.clone()),
                work_establishment_cash: work_establishment.map(|entry| entry.cash),
                work_establishment_stock: work_establishment
                    .map(|entry| entry.stock.clone())
                    .unwrap_or_default(),
                work_establishment_items: core
                    .work_building_id
                    .map(|building_id| self.establishment_item_stock_summaries(building_id, 8))
                    .unwrap_or_default(),
                local_prices: self.local_prices_for_agent(position.0),
                public_treasury: self.village_economy.public_treasury,
                political_position: self.political_position_for_agent(core.id),
                political_grievances: self.political_grievances_for_agent(core.id),
                feudal_title: self
                    .active_feudal_title_for_holder(core.id)
                    .map(|title| title.name.clone()),
                direct_lord_name: self
                    .direct_lord_for_agent(core.id)
                    .and_then(|id| agent_name_map.get(&id).cloned()),
                subordinate_names: self
                    .subordinates_for_agent(core.id)
                    .into_iter()
                    .filter_map(|id| agent_name_map.get(&id).cloned())
                    .collect(),
                feudal_obligations: self.build_feudal_context(core.id).obligations,
                feudal_power_summary: Some(self.build_feudal_context(core.id).power_summary),
                succession_status: self.build_feudal_context(core.id).succession_status,
                scheduled_meetings,
                planner_status: if self.planner_pending_for_agent(core.id) {
                    "pending".to_string()
                } else if intent.0.is_some() {
                    "ready".to_string()
                } else {
                    "idle".to_string()
                },
                active_utility_directive: utility.active.as_ref().map(|directive| {
                    format!(
                        "{} [{}] ({})",
                        directive.kind, directive.stance, directive.score
                    )
                }),
                reactive_stance: reactive_summary.stance.clone(),
                reactive_reason: reactive_summary.reason.clone(),
                reactive_revenge_target: reactive_summary
                    .target_agent_id
                    .and_then(|id| agent_name_map.get(&id).cloned()),
                reactive_status_pressure: reactive_summary.status_pressure_summary.clone(),
                reactive_defiance_posture: reactive_summary.defiance_posture_summary.clone(),
                control_mode: if utility.active.is_some() {
                    "utility".to_string()
                } else if intent.0.is_some() {
                    "planner".to_string()
                } else {
                    "idle".to_string()
                },
            });
        }
        views.sort_by(|a, b| a.name.cmp(&b.name));
        views
    }

    pub fn render_ascii_map(
        &mut self,
        selected_agent_id: Option<u64>,
        width: usize,
        height: usize,
    ) -> MapRender {
        let occupancy = self.occupancy_map();
        let engaged_agents = self.active_conversation_participants();
        let selected_path = selected_agent_id.and_then(|agent_id| self.agent_path(agent_id));
        let center = selected_agent_id
            .and_then(|agent_id| self.debug_agent_position(agent_id).ok())
            .unwrap_or(TileCoord {
                x: self.spatial.grid.width / 2,
                y: self.spatial.grid.height / 2,
            });

        let half_w = width as i32 / 2;
        let half_h = height as i32 / 2;
        let mut rows = Vec::new();

        for y in (center.y - half_h)..(center.y - half_h + height as i32) {
            let mut row = String::new();
            for x in (center.x - half_w)..(center.x - half_w + width as i32) {
                let coord = TileCoord { x, y };
                let mut ch = if let Some(tile) = self.tile_at(coord) {
                    tile.kind.glyph()
                } else {
                    ' '
                };

                if let Some(crop) = self.crops.get(&coord) {
                    ch = match crop.stage {
                        CropStage::Planted => '.',
                        CropStage::Growing => 'v',
                        CropStage::Ready => 'Y',
                    };
                }

                if let Some(fixture) = self.fixture_at(coord) {
                    ch = fixture.kind.glyph();
                }
                if let Some(path) = &selected_path {
                    if path.contains(&coord) {
                        ch = '*';
                    }
                }
                if let Some(agent_id) = occupancy.get(&coord) {
                    ch = if Some(*agent_id) == selected_agent_id {
                        '@'
                    } else if self
                        .life_status(*agent_id)
                        .map(|status| status == AgentLifeStatus::Morto)
                        .unwrap_or(false)
                    {
                        'x'
                    } else if engaged_agents.contains(agent_id) {
                        '&'
                    } else {
                        self.agent_initial(*agent_id).unwrap_or('a')
                    };
                }
                row.push(ch);
            }
            rows.push(row);
        }

        MapRender { rows }
    }

    pub(super) fn conversation_map(&self) -> HashMap<ConversationId, ConversationState> {
        self.conversations
            .iter()
            .map(|conversation| (conversation.id, conversation.clone()))
            .collect()
    }

    pub(super) fn active_conversation_participants(&self) -> HashSet<u64> {
        self.conversations
            .iter()
            .filter(|conversation| conversation.status == ConversationStatus::Active)
            .flat_map(|conversation| conversation.participant_ids.clone())
            .collect()
    }

    pub(super) fn agent_path(&mut self, agent_id: u64) -> Option<Vec<TileCoord>> {
        let entity = self.find_agent_entity(agent_id).ok()?;
        self.world
            .entity(entity)
            .get::<PathComponent>()
            .map(|path| path.0.clone())
    }
}
