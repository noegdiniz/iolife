use super::*;
// Conversation protocol, relationship updates and social memory systems.

impl Simulation {
    pub(super) fn build_cultural_context(
        &mut self,
        agent_id: u64,
        listener_id: Option<u64>,
    ) -> crate::agent_mind::CulturalContextInput {
        let beliefs = self
            .story_beliefs(agent_id)
            .into_iter()
            .filter(|belief| belief.belief >= 15)
            .collect::<Vec<_>>();
        let home_id = self.household_id_for_agent_immutable(agent_id);
        let position = self.debug_agent_position(agent_id).ok();
        let current_building_id =
            position.and_then(|pos| self.tile_at(pos).and_then(|tile| tile.building_id));
        let mut known_stories = Vec::new();
        let mut locally_relevant_stories = Vec::new();
        let mut family_stories = Vec::new();
        let mut stories_likely_to_tell = Vec::new();
        let mut cultural_risks = Vec::new();

        for belief in beliefs.iter().take(8) {
            if let Some(story) = self
                .cultural_stories
                .iter()
                .find(|story| story.id == belief.story_id)
                .cloned()
            {
                let line = format!(
                    "#{} {} | {:?}/{:?} | crenca={} apego={} forca={} distorcao={} moral={}",
                    story.id,
                    story.title,
                    story.origin_kind,
                    story.status,
                    belief.belief,
                    belief.emotional_attachment,
                    story.cultural_strength,
                    story.distortion,
                    belief.moral_interpretation
                );
                known_stories.push(line.clone());
                if story.associated_building_id == current_building_id
                    || story
                        .associated_building_id
                        .is_some_and(|building_id| Some(building_id) == home_id)
                {
                    locally_relevant_stories.push(line.clone());
                }
                if story
                    .cited_agent_ids
                    .iter()
                    .any(|cited| self.agent_is_family(agent_id, *cited))
                {
                    family_stories.push(line.clone());
                }
                if belief.belief + belief.emotional_attachment + story.cultural_strength / 2 >= 85
                    && !matches!(story.status, StoryStatus::Esquecida)
                {
                    stories_likely_to_tell.push(line);
                }
            }
        }

        if let Some(listener_id) = listener_id {
            let relation = self.relation_between(agent_id, listener_id);
            if relation.resentment > 35 {
                cultural_risks.push(
                    "historia familiar ou de faccao pode soar como provocacao ao ouvinte"
                        .to_string(),
                );
            }
        }
        if known_stories.is_empty() {
            cultural_risks.push("nenhuma historia cultural saliente conhecida".to_string());
        }
        crate::agent_mind::CulturalContextInput {
            known_stories,
            locally_relevant_stories,
            family_stories,
            faction_stories: Vec::new(),
            stories_likely_to_tell,
            cultural_risks,
        }
    }

    pub(super) fn story_beliefs(&mut self, agent_id: u64) -> Vec<StoryBelief> {
        let Ok(entity) = self.find_agent_entity(agent_id) else {
            return Vec::new();
        };
        self.world
            .entity(entity)
            .get::<StoryBeliefComponent>()
            .map(|component| component.0.clone())
            .unwrap_or_default()
    }

    fn upsert_story_belief(
        &mut self,
        agent_id: u64,
        story_id: CulturalStoryId,
        heard_from: Option<u64>,
        belief_delta: i32,
        attachment_delta: i32,
        moral_interpretation: String,
    ) -> Result<StoryBelief> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entry = self.world.entity_mut(entity);
        let mut component = entry
            .get_mut::<StoryBeliefComponent>()
            .ok_or_else(|| anyhow!("missing story belief component"))?;
        if let Some(existing) = component
            .0
            .iter_mut()
            .find(|belief| belief.story_id == story_id)
        {
            existing.belief = (existing.belief + belief_delta).clamp(0, 100);
            existing.emotional_attachment =
                (existing.emotional_attachment + attachment_delta).clamp(0, 100);
            existing.heard_from = heard_from;
            existing.last_heard_tick = self.total_ticks;
            if !moral_interpretation.trim().is_empty() {
                existing.moral_interpretation = moral_interpretation;
            }
            return Ok(existing.clone());
        }
        let belief = StoryBelief {
            story_id,
            belief: belief_delta.clamp(0, 100),
            emotional_attachment: attachment_delta.clamp(0, 100),
            moral_interpretation,
            heard_from,
            first_heard_tick: self.total_ticks,
            last_heard_tick: self.total_ticks,
        };
        component.0.push(belief.clone());
        Ok(belief)
    }

    fn agent_is_family(&mut self, agent_id: u64, other_id: u64) -> bool {
        if agent_id == other_id {
            return true;
        }
        let Ok(entity) = self.find_agent_entity(agent_id) else {
            return false;
        };
        let entry = self.world.entity(entity);
        let Some(lineage) = entry.get::<LineageComponent>() else {
            return false;
        };
        lineage.parents.contains(&other_id)
            || lineage.children.contains(&other_id)
            || lineage.spouse == Some(other_id)
    }

    pub(super) fn build_information_context(
        &mut self,
        agent_id: u64,
        listener_id: Option<u64>,
    ) -> InformationContextInput {
        let beliefs = self
            .rumor_beliefs(agent_id)
            .into_iter()
            .filter(|belief| belief.belief >= 20)
            .take(6)
            .collect::<Vec<_>>();
        let mut known_rumors = Vec::new();
        let mut believed_rumors = Vec::new();
        let mut credibility_notes = Vec::new();
        let mut slander_risks = Vec::new();
        for belief in beliefs {
            if let Some(rumor) = self.rumors.iter().find(|rumor| rumor.id == belief.rumor_id) {
                let line = format!(
                    "#{} {} | crenca={} verdade={} distorcao={} fontes={}",
                    rumor.id,
                    rumor.claim,
                    belief.belief,
                    rumor.truth_score,
                    rumor.distortion,
                    rumor.known_by.len()
                );
                known_rumors.push(line.clone());
                if belief.belief >= 55 {
                    believed_rumors.push(line);
                }
                if rumor.is_slander && !rumor.is_confirmed {
                    slander_risks.push(format!(
                        "repetir rumor #{} contra agente {} pode virar calunia",
                        rumor.id, rumor.target_agent_id
                    ));
                }
            }
        }
        let known_secrets = self
            .secrets
            .iter()
            .filter(|secret| secret.known_by.contains(&agent_id))
            .take(5)
            .map(|secret| format!("#{} {:?}: {}", secret.id, secret.kind, secret.summary))
            .collect::<Vec<_>>();
        if let Some(listener_id) = listener_id {
            let relation = self.relation_between(listener_id, agent_id);
            credibility_notes.push(format!(
                "o ouvinte tende a ponderar o falante com trust={} reputacao={} ressentimento={}",
                relation.trust, relation.reputation, relation.resentment
            ));
        }
        if known_rumors.is_empty() {
            credibility_notes.push("sem rumores relevantes conhecidos".to_string());
        }
        InformationContextInput {
            known_rumors,
            believed_rumors,
            known_secrets,
            credibility_notes,
            slander_risks,
        }
    }

    pub(super) fn rumor_beliefs(&mut self, agent_id: u64) -> Vec<RumorBelief> {
        let Ok(entity) = self.find_agent_entity(agent_id) else {
            return Vec::new();
        };
        self.world
            .entity(entity)
            .get::<RumorBeliefComponent>()
            .map(|component| component.0.clone())
            .unwrap_or_default()
    }

    fn upsert_rumor_belief(
        &mut self,
        agent_id: u64,
        rumor_id: u64,
        heard_from: Option<u64>,
        belief_delta: i32,
        skepticism_delta: i32,
    ) -> Result<RumorBelief> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entry = self.world.entity_mut(entity);
        let mut component = entry
            .get_mut::<RumorBeliefComponent>()
            .ok_or_else(|| anyhow!("missing rumor belief component"))?;
        if let Some(existing) = component
            .0
            .iter_mut()
            .find(|belief| belief.rumor_id == rumor_id)
        {
            existing.belief = (existing.belief + belief_delta).clamp(0, 100);
            existing.skepticism = (existing.skepticism + skepticism_delta).clamp(0, 100);
            existing.heard_from = heard_from;
            existing.last_reinforced_tick = self.total_ticks;
            return Ok(existing.clone());
        }
        let belief = RumorBelief {
            rumor_id,
            belief: belief_delta.clamp(0, 100),
            skepticism: skepticism_delta.clamp(0, 100),
            heard_from,
            first_heard_tick: self.total_ticks,
            last_reinforced_tick: self.total_ticks,
        };
        component.0.push(belief.clone());
        Ok(belief)
    }

    fn base_rumor_belief(&mut self, rumor: &Rumor, sender_id: u64, listener_id: u64) -> i32 {
        let relation = self.relation_between(listener_id, sender_id);
        let speaker_factor = relation.trust / 3 + relation.reputation / 4 - relation.resentment / 4;
        let corroboration = (rumor.known_by.len() as i32 * 4).clamp(0, 20);
        let distortion_penalty = rumor.distortion / 3;
        (rumor.credibility_seed + speaker_factor + corroboration - distortion_penalty).clamp(0, 100)
    }

    fn deterministic_distortion_for_spread(
        &mut self,
        rumor: &Rumor,
        sender_id: u64,
        listener_id: u64,
    ) -> i32 {
        let relation = self.relation_between(sender_id, listener_id);
        let distrust = (-relation.trust).max(0) / 8 + relation.resentment.max(0) / 10;
        let spread_pressure = (rumor.spread_count as i32 / 2).clamp(0, 10);
        let chaos = self.agent_chaos_pressure(sender_id).unwrap_or(0) as i32 / 20;
        (2 + distrust + spread_pressure + chaos).clamp(1, 18)
    }

    fn distorted_claim(&self, rumor: &Rumor, distortion_delta: i32) -> String {
        if rumor.distortion + distortion_delta < 35 {
            rumor.claim.clone()
        } else if rumor.distortion + distortion_delta < 70 {
            format!("Dizem que {}", rumor.claim)
        } else {
            format!("A historia se exagerou: {}", rumor.claim)
        }
    }

    pub(super) fn decay_rumors_daily(&mut self) -> Result<()> {
        for rumor in &mut self.rumors {
            if rumor.is_confirmed || rumor.is_disproven {
                continue;
            }
            let durable = ["guerra", "crime", "fome", "corrup", "traicao"]
                .iter()
                .any(|needle| rumor.topic.contains(needle) || rumor.claim.contains(needle));
            let decay = if durable { 2 } else { 6 };
            rumor.credibility_seed = (rumor.credibility_seed - decay).clamp(0, 100);
            rumor.distortion = (rumor.distortion + 1).clamp(0, 100);
        }
        let mut query = self.world.query::<&mut RumorBeliefComponent>();
        for mut beliefs in query.iter_mut(&mut self.world) {
            for belief in &mut beliefs.0 {
                let reinforced_recently =
                    self.total_ticks.saturating_sub(belief.last_reinforced_tick)
                        <= u64::from(self.ticks_per_day);
                if !reinforced_recently {
                    belief.belief = (belief.belief - 5).clamp(0, 100);
                    belief.skepticism = (belief.skepticism + 3).clamp(0, 100);
                }
            }
        }
        Ok(())
    }

    pub(super) fn update_cultural_stories_daily(&mut self) -> Result<()> {
        self.seed_cultural_stories_from_strong_events()?;
        let mut generated_events = Vec::new();
        for story in &mut self.cultural_stories {
            if matches!(story.status, StoryStatus::Esquecida) {
                continue;
            }
            let durable = matches!(
                story.origin_kind,
                CulturalStoryKind::CantoDeGuerra
                    | CulturalStoryKind::Martirio
                    | CulturalStoryKind::Fundacao
                    | CulturalStoryKind::Traicao
                    | CulturalStoryKind::Heroismo
            );
            let told_recently = self.total_ticks.saturating_sub(story.last_told_tick)
                <= u64::from(self.ticks_per_day * 2);
            if told_recently {
                story.stability = (story.stability + 3).clamp(0, 100);
                if story.distortion <= 35 {
                    story.cultural_strength = (story.cultural_strength + 2).clamp(0, 100);
                }
            } else {
                let decay = if durable { 1 } else { 4 };
                story.cultural_strength = (story.cultural_strength - decay).clamp(0, 100);
                story.stability = (story.stability - 1).clamp(0, 100);
            }
            if story.cultural_strength <= 4 && story.tell_count > 0 {
                story.status = StoryStatus::Esquecida;
                generated_events.push(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: 0,
                    target: None,
                    kind: EventKind::CulturalStory,
                    summary: format!("A historia '{}' saiu da memoria viva da vila.", story.title),
                    impact_tags: vec!["cultura".to_string(), "esquecimento".to_string()],
                });
            } else if story.tell_count >= 12
                && story.cultural_strength >= 75
                && story.distortion <= 35
            {
                story.status = StoryStatus::Canonizada;
            } else if story.tell_count >= 5 && story.stability >= 45 {
                story.status = StoryStatus::Estavel;
            }
        }
        for event in generated_events {
            self.push_event(event);
        }
        let mut query = self.world.query::<&mut StoryBeliefComponent>();
        for mut beliefs in query.iter_mut(&mut self.world) {
            for belief in &mut beliefs.0 {
                let heard_recently = self.total_ticks.saturating_sub(belief.last_heard_tick)
                    <= u64::from(self.ticks_per_day * 3);
                if !heard_recently {
                    belief.belief = (belief.belief - 2).clamp(0, 100);
                    belief.emotional_attachment = (belief.emotional_attachment - 1).clamp(0, 100);
                }
            }
        }
        Ok(())
    }

    fn seed_cultural_stories_from_strong_events(&mut self) -> Result<()> {
        let candidates = self
            .events
            .iter()
            .filter(|event| event.day == self.day.saturating_sub(1) || event.day == self.day)
            .filter_map(|event| {
                let kind = match event.kind {
                    EventKind::Death => Some(CulturalStoryKind::Martirio),
                    EventKind::Violence | EventKind::Punishment => {
                        Some(CulturalStoryKind::AdvertenciaMoral)
                    }
                    EventKind::MilitarySupply | EventKind::InstitutionalDispute
                        if event.impact_tags.iter().any(|tag| tag.contains("guerra")) =>
                    {
                        Some(CulturalStoryKind::CantoDeGuerra)
                    }
                    EventKind::Construction => Some(CulturalStoryKind::Fundacao),
                    EventKind::Theft if event.summary.to_lowercase().contains("tra") => {
                        Some(CulturalStoryKind::Traicao)
                    }
                    _ => None,
                }?;
                Some((event.clone(), kind))
            })
            .collect::<Vec<_>>();

        for (event, kind) in candidates {
            if self.cultural_stories.iter().any(|story| {
                story
                    .source_event_summaries
                    .iter()
                    .any(|summary| summary == &event.summary)
            }) {
                continue;
            }
            let story_id = self.next_cultural_story_id;
            self.next_cultural_story_id += 1;
            let actor_name = if event.actor == 0 {
                "a vila".to_string()
            } else {
                self.agent_name(event.actor)
                    .unwrap_or_else(|_| format!("Agente {}", event.actor))
            };
            let title = match kind {
                CulturalStoryKind::Martirio => format!("O martirio de {actor_name}"),
                CulturalStoryKind::CantoDeGuerra => format!("O canto de guerra de {actor_name}"),
                CulturalStoryKind::Fundacao => "A fundacao que mudou a vila".to_string(),
                CulturalStoryKind::Traicao => format!("A traicao lembrada de {actor_name}"),
                CulturalStoryKind::AdvertenciaMoral => "A advertencia contada ao povo".to_string(),
                _ => format!("A historia de {actor_name}"),
            };
            let associated_building_id = self
                .debug_agent_position(event.actor)
                .ok()
                .and_then(|pos| self.tile_at(pos).and_then(|tile| tile.building_id));
            let story = CulturalStory {
                id: story_id,
                title: title.clone(),
                narrative_core: event.summary.clone(),
                origin_kind: kind,
                theme: self.story_theme_from_kind(kind).to_string(),
                moral: self.story_moral_from_kind(kind).to_string(),
                cited_agent_ids: event
                    .target
                    .into_iter()
                    .chain([event.actor])
                    .filter(|id| *id != 0)
                    .collect(),
                associated_building_id,
                associated_territory_id: None,
                source_event_summaries: vec![event.summary.clone()],
                origin_generation: 0,
                cultural_strength: match kind {
                    CulturalStoryKind::Martirio | CulturalStoryKind::CantoDeGuerra => 35,
                    CulturalStoryKind::Fundacao => 26,
                    _ => 20,
                },
                stability: 12,
                distortion: 8,
                status: StoryStatus::Emergente,
                created_day: self.day,
                last_told_tick: self.total_ticks,
                tell_count: 0,
            };
            self.cultural_stories.push(story);
            self.story_versions.push(StoryVersion {
                id: self
                    .story_versions
                    .iter()
                    .map(|version| version.id)
                    .max()
                    .unwrap_or(0)
                    + 1,
                story_id,
                short_version: event.summary.clone(),
                author_agent_id: None,
                transmitter_agent_id: None,
                generation: 0,
                tone: "cronica".to_string(),
                distortion: 0,
                cultural_tags: event.impact_tags.clone(),
                created_day: self.day,
                created_tick: self.tick_of_day,
            });
            if event.actor != 0 {
                let _ = self.upsert_story_belief(
                    event.actor,
                    story_id,
                    None,
                    40,
                    35,
                    self.story_moral_from_kind(kind).to_string(),
                );
            }
            if let Some(target_id) = event.target {
                let _ = self.upsert_story_belief(
                    target_id,
                    story_id,
                    None,
                    35,
                    30,
                    self.story_moral_from_kind(kind).to_string(),
                );
            }
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: event.actor,
                target: event.target,
                kind: EventKind::CulturalStory,
                summary: format!(
                    "A vila começou a transformar '{}' em historia cultural.",
                    title
                ),
                impact_tags: vec!["cultura".to_string(), "semente".to_string()],
            });
        }
        Ok(())
    }

    pub(super) fn open_conversation(
        &mut self,
        initiator_id: u64,
        partner_id: u64,
        move_kind: SocialMove,
        reason: &str,
    ) -> Result<bool> {
        if !self.agents_adjacent(initiator_id, partner_id)? {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: initiator_id,
                target: Some(partner_id),
                kind: EventKind::Blocking,
                summary: "A conversa falha por falta de proximidade fisica.".to_string(),
                impact_tags: vec!["social".to_string(), "distancia".to_string()],
            });
            return Ok(false);
        }
        if self.agent_conversation_id(initiator_id)?.is_some()
            || self.agent_conversation_id(partner_id)?.is_some()
        {
            return Ok(false);
        }
        if self.agent_social_cooldown_until(initiator_id)? > self.total_ticks
            || self.agent_social_cooldown_until(partner_id)? > self.total_ticks
        {
            return Ok(false);
        }

        let conversation_id = self.next_conversation_id;
        self.next_conversation_id += 1;
        let initiator_name = self.agent_name(initiator_id)?;
        let partner_name = self.agent_name(partner_id)?;
        let opening_reason = format!("{}: {}", move_kind.as_str(), reason);
        self.conversations.push(ConversationState {
            id: conversation_id,
            participants: [initiator_id, partner_id],
            initiator_id,
            current_speaker_id: initiator_id,
            started_at_tick: self.total_ticks,
            turn_count: 0,
            max_turns: MAX_CONVERSATION_TURNS,
            opening_reason: opening_reason.clone(),
            summary: format!("{initiator_name} inicia uma conversa com {partner_name}."),
            recent_turns: Vec::new(),
            participant_states: vec![
                ConversationParticipantState {
                    agent_id: initiator_id,
                    social_goal: social_goal_from_move(move_kind).to_string(),
                    last_speech_act: None,
                    last_emotion: None,
                },
                ConversationParticipantState {
                    agent_id: partner_id,
                    social_goal: "entender a intencao do outro".to_string(),
                    last_speech_act: None,
                    last_emotion: None,
                },
            ],
            status: ConversationStatus::Active,
            outcome: ConversationOutcome::Ongoing,
            end_reason: None,
        });

        self.bind_agent_to_conversation(
            initiator_id,
            conversation_id,
            partner_id,
            format!("abre conversa para {}", move_kind.as_str()),
        )?;
        self.bind_agent_to_conversation(
            partner_id,
            conversation_id,
            initiator_id,
            "aceita conversa".to_string(),
        )?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: initiator_id,
            target: Some(partner_id),
            kind: EventKind::ConversationStarted,
            summary: format!("{initiator_name} inicia conversa com {partner_name}: {reason}."),
            impact_tags: vec![
                "social".to_string(),
                "conversa".to_string(),
                move_kind.as_str().to_string(),
            ],
        });
        Ok(true)
    }

    pub(super) fn process_active_conversations(&mut self, llm: &dyn LlmAdapter) -> Result<()> {
        let active_ids = self
            .conversations
            .iter()
            .filter(|conversation| conversation.status == ConversationStatus::Active)
            .map(|conversation| conversation.id)
            .collect::<Vec<_>>();
        let mut prepared_turns = Vec::new();
        for conversation_id in active_ids {
            let Some(conversation) = self.conversation_state(conversation_id) else {
                continue;
            };
            if conversation.status != ConversationStatus::Active {
                continue;
            }
            if let Some((status, outcome, reason)) =
                self.conversation_interruption(&conversation)?
            {
                self.end_conversation(conversation_id, status, outcome, reason)?;
                continue;
            }

            let speaker_id = conversation.current_speaker_id;
            let listener_id = other_participant(&conversation.participants, speaker_id);
            let input =
                self.build_conversation_turn_input(&conversation, speaker_id, listener_id)?;
            prepared_turns.push(PreparedConversationTurn {
                conversation_id,
                speaker_id,
                listener_id,
                input,
            });
        }
        if prepared_turns.is_empty() {
            return Ok(());
        }

        let turn_results = self.run_parallel_conversation_turns(llm, prepared_turns)?;
        for result in turn_results {
            match result {
                ConversationBatchItem::Completed(result) => {
                    self.apply_conversation_turn_output(
                        result.conversation_id,
                        result.speaker_id,
                        result.listener_id,
                        result.output,
                    )?;
                }
                ConversationBatchItem::Interrupted(result) => {
                    self.handle_transient_conversation_failure(
                        result.conversation_id,
                        result.speaker_id,
                        result.listener_id,
                        &result.error,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn conversation_interruption(
        &mut self,
        conversation: &ConversationState,
    ) -> Result<Option<(ConversationStatus, ConversationOutcome, String)>> {
        let [agent_a, agent_b] = conversation.participants;
        if !self.agents_adjacent(agent_a, agent_b)? {
            return Ok(Some((
                ConversationStatus::Interrupted,
                ConversationOutcome::DistanceBreak,
                "os participantes perderam adjacencia".to_string(),
            )));
        }
        for agent_id in conversation.participants {
            let state = self.agent_state(agent_id)?;
            if state.hunger >= 95 || state.energy <= 5 || state.health <= 15 {
                return Ok(Some((
                    ConversationStatus::Interrupted,
                    ConversationOutcome::CriticalNeed,
                    format!(
                        "{} abandona a conversa por necessidade critica.",
                        self.agent_name(agent_id)?
                    ),
                )));
            }
        }
        Ok(None)
    }

    pub(super) fn build_conversation_turn_input(
        &mut self,
        conversation: &ConversationState,
        speaker_id: u64,
        listener_id: u64,
    ) -> Result<ConversationTurnInput> {
        let speaker_entity = self.find_agent_entity(speaker_id)?;
        let listener_entity = self.find_agent_entity(listener_id)?;
        let (
            speaker_name,
            speaker_role,
            speaker_position,
            speaker_state,
            speaker_profile,
            speaker_memories,
        ) = {
            let entry = self.world.entity(speaker_entity);
            (
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .name
                    .clone(),
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .role_id
                    .clone(),
                entry
                    .get::<PositionComponent>()
                    .ok_or_else(|| anyhow!("missing position component"))?
                    .0,
                entry
                    .get::<StateComponent>()
                    .ok_or_else(|| anyhow!("missing state component"))?
                    .0
                    .clone(),
                entry
                    .get::<ProfileComponent>()
                    .ok_or_else(|| anyhow!("missing profile component"))?
                    .0
                    .clone(),
                entry
                    .get::<MemoryComponent>()
                    .ok_or_else(|| anyhow!("missing memory component"))?
                    .0
                    .clone(),
            )
        };
        let (listener_name, listener_role, listener_state) = {
            let entry = self.world.entity(listener_entity);
            (
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .name
                    .clone(),
                entry
                    .get::<AgentCore>()
                    .ok_or_else(|| anyhow!("missing agent core"))?
                    .role_id
                    .clone(),
                entry
                    .get::<StateComponent>()
                    .ok_or_else(|| anyhow!("missing state component"))?
                    .0
                    .clone(),
            )
        };
        let recent_events =
            self.recent_events_for(speaker_id, speaker_position, self.recent_event_limit);
        let recent_memories = retrieve_relational_memories(&speaker_memories, listener_id, 5);
        let tile = self.tile_at(speaker_position);
        let current_building = tile
            .and_then(|entry| entry.building_id)
            .and_then(|id| self.building_name(id));
        let current_room = tile
            .and_then(|entry| entry.room_id)
            .and_then(|id| self.room_name(id));
        let agent_name_map = self.agent_name_map();
        let recent_turns = conversation
            .recent_turns
            .iter()
            .map(|turn| {
                format!(
                    "{} [{}]: {}",
                    agent_name_map
                        .get(&turn.speaker_id)
                        .cloned()
                        .unwrap_or_else(|| format!("Agente {}", turn.speaker_id)),
                    turn.speech_act,
                    turn.utterance
                )
            })
            .collect::<Vec<_>>();
        let relation = self.relation_between(speaker_id, listener_id);
        let speaker_psychological_state = self.psychological_state_for_agent(speaker_id)?;
        let speaker_psychology = self.build_psychological_context_for_values(
            speaker_id,
            &speaker_profile,
            &speaker_state,
            &speaker_memories,
            &recent_events,
            &recent_memories,
            "conversa_ativa",
            &speaker_psychological_state,
        );
        let listener_memories = self.agent_memories(listener_id)?;
        let listener_recent_events =
            self.recent_events_for(listener_id, speaker_position, self.recent_event_limit);
        let listener_relevant_memories =
            retrieve_relational_memories(&listener_memories, speaker_id, 5);
        let listener_profile = self.agent_profile(listener_id)?;
        let listener_psychological_state = self.psychological_state_for_agent(listener_id)?;
        let listener_psychology = self.build_psychological_context_for_values(
            listener_id,
            &listener_profile,
            &listener_state,
            &listener_memories,
            &listener_recent_events,
            &listener_relevant_memories,
            "observando_conversa",
            &listener_psychological_state,
        );
        let relational_context =
            self.build_relational_history(speaker_id, listener_id, &relation, &speaker_memories);
        let speaker_injury = self.agent_injury(speaker_id)?;
        let reactive_summary = self.current_reactive_psychology_summary(speaker_id)?;

        Ok(ConversationTurnInput {
            speaker_id,
            speaker_name,
            speaker_role: self.role_display_name(&speaker_role),
            speaker_state: speaker_state.clone(),
            time_context: self.time_context(),
            world_places: self.world_place_inputs(),
            speaker_profile_summary: {
                let mut summary = speaker_profile.values.clone();
                summary.extend(speaker_profile.long_term_desires.clone());
                summary.extend(speaker_profile.fears.clone());
                summary
            },
            speaker_psychology,
            speaker_equipment_summary: self.visible_equipment_summary(speaker_id),
            speaker_prestige_summary: self.visible_prestige_summary(speaker_id),
            speaker_prestige_score: self.perceived_status_score(speaker_id),
            reactive_stance: reactive_summary.stance.clone(),
            status_pressure_summary: reactive_summary.status_pressure_summary.clone(),
            revenge_summary: reactive_summary.revenge_summary.clone(),
            public_shame_summary: reactive_summary.public_shame_summary.clone(),
            authority_posture_summary: reactive_summary.authority_posture_summary.clone(),
            prestige_gap_summary: reactive_summary.prestige_gap_summary.clone(),
            humiliation_risk_summary: reactive_summary.humiliation_risk_summary.clone(),
            deference_or_revenge_summary: reactive_summary.deference_or_revenge_summary.clone(),
            audience_summary: reactive_summary.audience_summary.clone(),
            listener: ConversationObservedAgentInput {
                id: listener_id,
                name: listener_name,
                role: self.role_display_name(&listener_role),
                state: listener_state,
                relation,
                perceived_status: self.visible_prestige_summary(listener_id),
                visible_equipment_summary: self.visible_equipment_summary(listener_id),
                psychological_summary: listener_psychology,
            },
            context: ConversationContextInput {
                conversation_id: conversation.id,
                opening_reason: conversation.opening_reason.clone(),
                current_area: self.area_name(speaker_position),
                current_building,
                current_room,
                max_turns: conversation.max_turns,
                turn_count: conversation.turn_count,
                turns_remaining: conversation
                    .max_turns
                    .saturating_sub(conversation.turn_count),
                conversation_summary: conversation.summary.clone(),
            },
            turn_trigger: "fala_social".to_string(),
            relational_context,
            recent_memories,
            recent_turns: recent_turns
                .into_iter()
                .rev()
                .take(CONVERSATION_RECENT_TURNS_LIMIT)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect(),
            chaos_pressure: self.agent_chaos_pressure(speaker_id).unwrap_or(0),
            personality_traits: speaker_profile.traits.clone(),
            trauma_traits: speaker_profile.trauma_traits.clone(),
            known_secrets: self
                .secrets
                .iter()
                .filter(|s| s.known_by.contains(&speaker_id))
                .map(|s| format!("ID: {} - {} Detalhes: {}", s.id, s.summary, s.details))
                .collect(),
            information_context: self.build_information_context(speaker_id, Some(listener_id)),
            cultural_context: self.build_cultural_context(speaker_id, Some(listener_id)),
            body_parts: speaker_injury.body_parts.clone(),
        })
    }

    pub(super) fn apply_conversation_turn_output(
        &mut self,
        conversation_id: ConversationId,
        speaker_id: u64,
        listener_id: u64,
        output: crate::agent_mind::ConversationTurnOutput,
    ) -> Result<()> {
        let speaker_name = self.agent_name(speaker_id)?;
        let listener_name = self.agent_name(listener_id)?;
        let turn = ConversationTurn {
            speaker_id,
            listener_id,
            tick: self.total_ticks,
            utterance: output.utterance.clone(),
            speech_act: output.speech_act.clone(),
            emotion: output.emotion.clone(),
            tone: output.tone.clone(),
        };

        self.apply_relation_delta(speaker_id, listener_id, &output.relation_delta_hint)?;
        self.apply_relation_delta(
            listener_id,
            speaker_id,
            &invert_delta(&output.relation_delta_hint),
        )?;
        self.apply_conversation_effects(
            speaker_id,
            listener_id,
            &output.speech_act,
            &output.emotion,
            output.risk_shift.unwrap_or(0),
            &output.belief_updates,
        )?;

        // Propagação do Edital do Rei / Telefone Sem Fio
        let mut editais_para_passar = Vec::new();
        let editais_ativos = self.active_edict_tags();

        if !editais_ativos.is_empty() {
            let speaker_role = self.agent_role_id(speaker_id)?;
            let speaker_memories = self.agent_memories(speaker_id)?;
            let listener_memories = self.agent_memories(listener_id)?;

            for edital in editais_ativos {
                let speaker_sabe = speaker_role == "lider_local"
                    || speaker_role == "guarda"
                    || speaker_memories.iter().any(|m| m.tags.contains(&edital));

                if speaker_sabe {
                    let listener_sabe = listener_memories.iter().any(|m| m.tags.contains(&edital));
                    if !listener_sabe {
                        editais_para_passar.push(edital);
                    }
                }
            }
        }

        for edital in editais_para_passar {
            let edital_desc = match edital.as_str() {
                "trabalho_forcado_campos" => "Trabalho Forcado nos Campos",
                "racionamento_estrito" => "Racionamento Estrito de Graos",
                "imposto_guerra" => "Imposto de Guerra Dobrado",
                "proibicao_tavernas" => "Proibicao de Tavernas",
                "confisco_metais" => "Confisco Geral de Metais",
                _ => edital.as_str(),
            };

            self.add_memory(
                listener_id,
                MemoryKind::Fact,
                format!(
                    "Ouvi de {} que o Rei decretou o edital: {}",
                    speaker_name, edital_desc
                ),
                vec!["edital_rei".to_string(), edital.clone()],
                5,
                vec![speaker_id],
            )?;

            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: speaker_id,
                target: Some(listener_id),
                kind: EventKind::SocialBond,
                summary: format!(
                    "{} espalhou a noticia do edital '{}' para {}",
                    speaker_name, edital, listener_name
                ),
                impact_tags: vec![
                    "edital_rei".to_string(),
                    "telefone_sem_fio".to_string(),
                    edital.clone(),
                ],
            });

            // Avaliar resistência psicológica imediatamente no listener ao saber da notícia!
            self.apply_edict_psychological_resistance(listener_id, &edital)?;
        }

        // Conspiração do Mercado Negro
        let speaker_role = self.agent_role_id(speaker_id)?;
        let listener_role = self.agent_role_id(listener_id)?;

        let comum = speaker_role != "lider_local"
            && speaker_role != "guarda"
            && listener_role != "lider_local"
            && listener_role != "guarda";

        if comum {
            let speaker_state = self.agent_state(speaker_id)?;
            let listener_state = self.agent_state(listener_id)?;
            let speaker_memories = self.agent_memories(speaker_id)?;

            let speaker_conhece = speaker_memories
                .iter()
                .any(|m| m.tags.contains(&"edital_rei".to_string()));
            let estressados = speaker_state.stress >= 50 || listener_state.stress >= 50;

            if speaker_conhece && estressados {
                let speaker_tem_mn = speaker_memories
                    .iter()
                    .any(|m| m.tags.contains(&"mercado_negro".to_string()));
                let listener_memories = self.agent_memories(listener_id)?;
                let listener_tem_mn = listener_memories
                    .iter()
                    .any(|m| m.tags.contains(&"mercado_negro".to_string()));

                if !speaker_tem_mn || !listener_tem_mn {
                    if !speaker_tem_mn {
                        self.add_memory(
                            speaker_id,
                            MemoryKind::Fact,
                            "Nos decidimos conspirar contra as leis do Rei e planejar o contrabando.".to_string(),
                            vec!["mercado_negro".to_string(), "conspiracao".to_string()],
                            8,
                            vec![listener_id],
                        )?;
                    }
                    if !listener_tem_mn {
                        self.add_memory(
                            listener_id,
                            MemoryKind::Fact,
                            "Nos decidimos conspirar contra as leis do Rei e planejar o contrabando.".to_string(),
                            vec!["mercado_negro".to_string(), "conspiracao".to_string()],
                            8,
                            vec![speaker_id],
                        )?;
                    }

                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: speaker_id,
                        target: Some(listener_id),
                        kind: EventKind::SocialBond,
                        summary: format!(
                            "{} e {} sussurraram e conspiraram sobre o mercado negro na taverna",
                            speaker_name, listener_name
                        ),
                        impact_tags: vec!["mercado_negro".to_string(), "conspiracao".to_string()],
                    });
                }
            }
        }

        let (should_end, end_status, end_outcome, end_reason) = {
            let conversation = self
                .conversation_state_mut(conversation_id)
                .ok_or_else(|| anyhow!("conversation {conversation_id} not found"))?;
            conversation.turn_count += 1;
            conversation.summary = extend_summary(
                &conversation.summary,
                &format!("{speaker_name}: {}", output.utterance),
            );
            conversation.recent_turns.push(turn);
            if conversation.recent_turns.len() > CONVERSATION_RECENT_TURNS_LIMIT {
                let overflow = conversation.recent_turns.len() - CONVERSATION_RECENT_TURNS_LIMIT;
                conversation.recent_turns.drain(0..overflow);
            }
            if let Some(participant) = conversation
                .participant_states
                .iter_mut()
                .find(|participant| participant.agent_id == speaker_id)
            {
                participant.last_speech_act = Some(output.speech_act.clone());
                participant.last_emotion = Some(output.emotion.clone());
                if let Some(goal) = output.belief_updates.first() {
                    participant.social_goal = goal.clone();
                }
            }

            let should_end = if !output.intent_to_continue {
                Some((
                    ConversationStatus::Ended,
                    ConversationOutcome::OneSidedExit,
                    format!("{speaker_name} decide encerrar a conversa."),
                ))
            } else if conversation.turn_count >= conversation.max_turns {
                Some((
                    ConversationStatus::Ended,
                    ConversationOutcome::MaxTurns,
                    "a conversa atingiu o limite de turnos".to_string(),
                ))
            } else {
                conversation.current_speaker_id = listener_id;
                None
            };
            (
                should_end.is_some(),
                should_end
                    .as_ref()
                    .map(|tuple| tuple.0.clone())
                    .unwrap_or(ConversationStatus::Active),
                should_end
                    .as_ref()
                    .map(|tuple| tuple.1.clone())
                    .unwrap_or(ConversationOutcome::Ongoing),
                should_end.map(|tuple| tuple.2).unwrap_or_default(),
            )
        };

        self.set_last_social_act(speaker_id, output.speech_act.clone())?;
        self.set_last_social_act(listener_id, format!("ouve {}", output.speech_act))?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: speaker_id,
            target: Some(listener_id),
            kind: EventKind::ConversationTurn,
            summary: format!(
                "{speaker_name} fala com {listener_name}: {}",
                output.utterance
            ),
            impact_tags: vec![
                "social".to_string(),
                "conversa".to_string(),
                output.speech_act.clone(),
            ],
        });

        if let Some(ref transfer) = output.economic_transfer {
            self.execute_dialogue_economic_transfer(speaker_id, transfer)?;
        }
        if let Some(ref secret_reveal) = output.revealed_secret {
            self.execute_dialogue_secret_reveal(speaker_id, secret_reveal)?;
        }
        if let Some(ref promise) = output.make_promise {
            self.execute_dialogue_make_promise(speaker_id, promise)?;
        }
        if let Some(ref rumor) = output.spread_rumor {
            self.execute_dialogue_spread_rumor(speaker_id, listener_id, rumor)?;
        }
        if let Some(ref story) = output.shared_story {
            self.execute_dialogue_share_story(speaker_id, listener_id, story)?;
        }
        if let Some(ref escrow) = output.escrow_deposit {
            self.execute_dialogue_escrow_deposit(speaker_id, escrow)?;
        }
        if let Some(ref meeting) = output.propose_meeting {
            self.execute_dialogue_propose_meeting(speaker_id, listener_id, meeting)?;
        }
        if let Some(ref response) = output.meeting_response {
            self.execute_dialogue_meeting_response(speaker_id, response)?;
        }

        self.apply_faction_recruitment(speaker_id, listener_id)?;

        if should_end {
            self.end_conversation(conversation_id, end_status, end_outcome, end_reason)?;
        }
        Ok(())
    }

    pub(super) fn process_scheduled_meetings(&mut self) -> Result<()> {
        let now_day = self.day;
        let now_tick = self.tick_of_day;
        let active_ids = self
            .scheduled_meetings
            .iter()
            .filter(|meeting| meeting.status == ScheduledMeetingStatus::Active)
            .map(|meeting| meeting.id)
            .collect::<Vec<_>>();
        for meeting_id in active_ids {
            if let Some(meeting) = self
                .scheduled_meetings
                .iter()
                .find(|meeting| meeting.id == meeting_id)
                .cloned()
            {
                if !self.conversation_between_active(meeting.proposer_id, meeting.invitee_id) {
                    self.set_meeting_status(meeting_id, ScheduledMeetingStatus::Completed, None)?;
                }
            }
        }

        let accepted = self
            .scheduled_meetings
            .iter()
            .filter(|meeting| meeting.status == ScheduledMeetingStatus::Accepted)
            .cloned()
            .collect::<Vec<_>>();
        for meeting in accepted {
            let due_soon = meeting.scheduled_day == now_day
                && meeting.scheduled_tick >= now_tick
                && meeting.scheduled_tick.saturating_sub(now_tick) <= 30;
            if due_soon {
                self.queue_meeting_travel_task(meeting.proposer_id, &meeting.place_id)?;
                self.queue_meeting_travel_task(meeting.invitee_id, &meeting.place_id)?;
            }

            let due = meeting.scheduled_day < now_day
                || (meeting.scheduled_day == now_day && meeting.scheduled_tick <= now_tick);
            if !due {
                continue;
            }
            if self.participant_near_place(meeting.proposer_id, &meeting.place_id)?
                && self.participant_near_place(meeting.invitee_id, &meeting.place_id)?
                && self.agents_adjacent(meeting.proposer_id, meeting.invitee_id)?
            {
                if self.open_conversation(
                    meeting.proposer_id,
                    meeting.invitee_id,
                    SocialMove::Chat,
                    "encontro_marcado",
                )? {
                    self.set_meeting_status(meeting.id, ScheduledMeetingStatus::Active, None)?;
                }
            } else if meeting.scheduled_day < now_day
                || now_tick.saturating_sub(meeting.scheduled_tick) > 30
            {
                self.mark_meeting_missed(meeting.id)?;
            }
        }
        Ok(())
    }

    pub(super) fn execute_dialogue_propose_meeting(
        &mut self,
        speaker_id: u64,
        listener_id: u64,
        meeting: &ProposedMeeting,
    ) -> Result<()> {
        if meeting.invitee_id != listener_id || self.place_by_id(&meeting.place_id).is_none() {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: speaker_id,
                target: Some(listener_id),
                kind: EventKind::CognitionFailure,
                summary: format!(
                    "Proposta de encontro ignorada por contrato invalido: place_id={} invitee_id={}",
                    meeting.place_id, meeting.invitee_id
                ),
                impact_tags: vec!["encontro".to_string(), "place_id_invalido".to_string()],
            });
            return Ok(());
        }
        let Some(scheduled_tick) = self.parse_scheduled_time_to_tick(&meeting.scheduled_time)
        else {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: speaker_id,
                target: Some(listener_id),
                kind: EventKind::CognitionFailure,
                summary: format!(
                    "Proposta de encontro ignorada por horario invalido: {}",
                    meeting.scheduled_time
                ),
                impact_tags: vec!["encontro".to_string(), "horario_invalido".to_string()],
            });
            return Ok(());
        };
        if !self.meeting_time_is_future(meeting.scheduled_day, scheduled_tick) {
            return Ok(());
        }
        let id = self.next_scheduled_meeting_id;
        self.next_scheduled_meeting_id += 1;
        self.scheduled_meetings.push(ScheduledMeeting {
            id,
            proposer_id: speaker_id,
            invitee_id: listener_id,
            place_id: meeting.place_id.clone(),
            scheduled_day: meeting.scheduled_day,
            scheduled_tick,
            purpose: meeting.purpose.clone(),
            status: ScheduledMeetingStatus::Proposed,
            created_tick: self.total_ticks,
            response_tick: None,
        });
        let speaker_name = self.agent_name(speaker_id)?;
        let listener_name = self.agent_name(listener_id)?;
        let place_name = self
            .place_by_id(&meeting.place_id)
            .map(|place| place.display_name)
            .unwrap_or_else(|| meeting.place_id.clone());
        self.add_memory(
            listener_id,
            MemoryKind::Fact,
            format!(
                "{} marcou um encontro comigo em {} no Dia {} as {}: {}",
                speaker_name,
                place_name,
                meeting.scheduled_day,
                meeting.scheduled_time,
                meeting.purpose
            ),
            vec!["encontro".to_string(), "agenda_social".to_string()],
            5,
            vec![speaker_id],
        )?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: speaker_id,
            target: Some(listener_id),
            kind: EventKind::Meeting,
            summary: format!(
                "{} propos encontro #{} com {} em {} no Dia {} tick {}: {}",
                speaker_name,
                id,
                listener_name,
                place_name,
                meeting.scheduled_day,
                scheduled_tick,
                meeting.purpose
            ),
            impact_tags: vec!["encontro".to_string(), "proposto".to_string()],
        });
        Ok(())
    }

    pub(super) fn execute_dialogue_meeting_response(
        &mut self,
        responder_id: u64,
        response: &MeetingResponse,
    ) -> Result<()> {
        let Some(meeting) = self
            .scheduled_meetings
            .iter()
            .find(|meeting| meeting.id == response.meeting_id)
            .cloned()
        else {
            return Ok(());
        };
        if meeting.invitee_id != responder_id || meeting.status != ScheduledMeetingStatus::Proposed
        {
            return Ok(());
        }
        let new_status = if response.accept {
            ScheduledMeetingStatus::Accepted
        } else {
            ScheduledMeetingStatus::Rejected
        };
        self.set_meeting_status(meeting.id, new_status, Some(self.total_ticks))?;
        if response.accept {
            self.queue_meeting_travel_task(meeting.proposer_id, &meeting.place_id)?;
            self.queue_meeting_travel_task(meeting.invitee_id, &meeting.place_id)?;
        }
        let responder_name = self.agent_name(responder_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: responder_id,
            target: Some(meeting.proposer_id),
            kind: EventKind::Meeting,
            summary: format!(
                "{} {} encontro #{}: {}",
                responder_name,
                if response.accept {
                    "aceitou"
                } else {
                    "recusou"
                },
                meeting.id,
                response.reason
            ),
            impact_tags: vec![
                "encontro".to_string(),
                if response.accept {
                    "aceito"
                } else {
                    "recusado"
                }
                .to_string(),
            ],
        });
        Ok(())
    }

    fn parse_scheduled_time_to_tick(&self, raw: &str) -> Option<u32> {
        let mut parts = raw.trim().split(':');
        let hour = parts.next()?.trim().parse::<u32>().ok()?;
        let minute = parts.next()?.trim().parse::<u32>().ok()?;
        if hour > 23 || minute > 59 {
            return None;
        }
        let minute_of_day = hour * 60 + minute;
        Some(((u64::from(minute_of_day) * u64::from(self.ticks_per_day.max(1))) / 1_440) as u32)
    }

    fn meeting_time_is_future(&self, scheduled_day: u32, scheduled_tick: u32) -> bool {
        scheduled_day > self.day || (scheduled_day == self.day && scheduled_tick > self.tick_of_day)
    }

    fn queue_meeting_travel_task(&mut self, agent_id: u64, place_id: &str) -> Result<()> {
        if self.place_by_id(place_id).is_none() {
            return Ok(());
        }
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut queue = entity_mut
            .get_mut::<TaskQueueComponent>()
            .ok_or_else(|| anyhow!("missing task queue component"))?;
        let already_queued = queue.0.iter().any(|task| {
            task.kind == IntentKind::Andar && task.target_semantic.as_deref() == Some(place_id)
        });
        if !already_queued {
            queue.0.push_back(SimplifiedTask {
                kind: IntentKind::Andar,
                target_semantic: Some(place_id.to_string()),
                target_agent: None,
                social_move: None,
            });
        }
        Ok(())
    }

    fn participant_near_place(&mut self, agent_id: u64, place_id: &str) -> Result<bool> {
        let position = self.debug_agent_position(agent_id)?;
        let Some(destination) = self.place_target_coord(place_id) else {
            return Ok(false);
        };
        Ok(position == destination || position.manhattan(destination) <= 2)
    }

    fn conversation_between_active(&self, a: u64, b: u64) -> bool {
        self.conversations.iter().any(|conversation| {
            conversation.status == ConversationStatus::Active
                && conversation.participants.contains(&a)
                && conversation.participants.contains(&b)
        })
    }

    fn set_meeting_status(
        &mut self,
        meeting_id: ScheduledMeetingId,
        status: ScheduledMeetingStatus,
        response_tick: Option<u64>,
    ) -> Result<()> {
        let Some(meeting) = self
            .scheduled_meetings
            .iter_mut()
            .find(|meeting| meeting.id == meeting_id)
        else {
            return Ok(());
        };
        meeting.status = status;
        if response_tick.is_some() {
            meeting.response_tick = response_tick;
        }
        let actor = meeting.proposer_id;
        let target = meeting.invitee_id;
        let id = meeting.id;
        let status_label = format!("{:?}", meeting.status);
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor,
            target: Some(target),
            kind: EventKind::Meeting,
            summary: format!("Encontro #{} agora esta {}", id, status_label),
            impact_tags: vec!["encontro".to_string(), status_label],
        });
        Ok(())
    }

    fn mark_meeting_missed(&mut self, meeting_id: ScheduledMeetingId) -> Result<()> {
        let Some(meeting) = self
            .scheduled_meetings
            .iter()
            .find(|meeting| meeting.id == meeting_id)
            .cloned()
        else {
            return Ok(());
        };
        self.set_meeting_status(meeting_id, ScheduledMeetingStatus::Missed, None)?;
        self.add_memory(
            meeting.proposer_id,
            MemoryKind::Failure,
            format!(
                "O encontro #{} foi perdido: {}",
                meeting.id, meeting.purpose
            ),
            vec!["encontro".to_string(), "perdido".to_string()],
            6,
            vec![meeting.invitee_id],
        )?;
        self.add_memory(
            meeting.invitee_id,
            MemoryKind::Failure,
            format!(
                "O encontro #{} foi perdido: {}",
                meeting.id, meeting.purpose
            ),
            vec!["encontro".to_string(), "perdido".to_string()],
            6,
            vec![meeting.proposer_id],
        )?;
        self.apply_relation_delta(
            meeting.proposer_id,
            meeting.invitee_id,
            &RelationDelta {
                trust: -1,
                friendship: 0,
                resentment: 1,
                attraction: 0,
                moral_debt: 0,
                reputation: -1,
            },
        )?;
        Ok(())
    }

    pub(super) fn execute_dialogue_economic_transfer(
        &mut self,
        sender_id: u64,
        transfer: &crate::agent_mind::EconomicTransfer,
    ) -> Result<()> {
        let recipient_id = match transfer.recipient_id {
            Some(rid) => rid,
            None => return Ok(()),
        };
        let amount = transfer.amount.max(0);
        if amount == 0 {
            return Ok(());
        }

        let sender_name = self.agent_name(sender_id)?;
        let recipient_name = self.agent_name(recipient_id)?;

        let mut success = false;
        let mut corruption_embezzle = false;

        // Check if the sender is a leader and chose to use public treasury
        if transfer.use_public_treasury {
            let role = {
                let ent = self.find_agent_entity(sender_id)?;
                self.world
                    .entity(ent)
                    .get::<AgentCore>()
                    .unwrap()
                    .role_id
                    .clone()
            };
            if role == "lider" {
                if self.village_economy.public_treasury >= amount {
                    self.village_economy.public_treasury -= amount;
                    success = true;
                    corruption_embezzle = true;
                } else {
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: sender_id,
                        target: Some(recipient_id),
                        kind: EventKind::SocialBond,
                        summary: format!("{} tentou subornar {} usando o erÃ¡rio pÃºblico, mas os fundos estavam vazios.", sender_name, recipient_name),
                        impact_tags: vec!["social".to_string(), "corrupcao".to_string(), "falha".to_string()],
                    });
                }
            }
        }

        if !success {
            // Personal transfer
            if let Ok(sender_ent) = self.find_agent_entity(sender_id) {
                let mut sender_entry = self.world.entity_mut(sender_ent);
                if let Some(mut inv) = sender_entry.get_mut::<InventoryComponent>() {
                    if let Some(money_stack) = inv
                        .0
                        .iter_mut()
                        .find(|s| s.resource_id == transfer.resource_id)
                    {
                        if money_stack.amount >= amount {
                            money_stack.amount -= amount;
                            success = true;
                        }
                    }
                }
            }
        }

        if success {
            // Deposit to recipient
            if let Ok(rec_ent) = self.find_agent_entity(recipient_id) {
                let mut rec_entry = self.world.entity_mut(rec_ent);
                if let Some(mut inv) = rec_entry.get_mut::<InventoryComponent>() {
                    if let Some(money_stack) = inv
                        .0
                        .iter_mut()
                        .find(|s| s.resource_id == transfer.resource_id)
                    {
                        money_stack.amount += amount;
                    } else {
                        inv.0.push(ResourceStack {
                            resource_id: transfer.resource_id.clone(),
                            amount,
                        });
                    }
                }
            }

            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: sender_id,
                target: Some(recipient_id),
                kind: EventKind::SocialBond,
                summary: format!(
                    "{} transferiu {} {} para {} em acordo conversacional.",
                    sender_name, amount, transfer.resource_id, recipient_name
                ),
                impact_tags: vec![
                    "social".to_string(),
                    "transacao".to_string(),
                    transfer.resource_id.clone(),
                ],
            });

            // If it was corruption, generate a Secret
            if corruption_embezzle {
                let secret_id = self.next_secret_id;
                self.next_secret_id += 1;

                let sender_pos = self.debug_agent_position(sender_id)?;
                let mut eyewitnesses = Vec::new();

                let mut query =
                    self.world
                        .query::<(Entity, &AgentCore, &PositionComponent, &LifeStatusComponent)>();
                for (ent, core, pos, status) in query.iter(&self.world) {
                    if status.0 == AgentLifeStatus::Vivo
                        && core.id != sender_id
                        && core.id != recipient_id
                    {
                        if pos.0.manhattan(sender_pos) <= 3 {
                            eyewitnesses.push(core.id);
                        }
                    }
                }

                let mut known_by = vec![sender_id, recipient_id];
                known_by.extend(eyewitnesses.clone());

                let secret = Secret {
                    id: secret_id,
                    kind: SecretKind::CorruptionEmbezzle,
                    target_id: sender_id, // Leader ID
                    summary: format!(
                        "O LÃ­der {} desviou {} moedas do ErÃ¡rio pÃºblico.",
                        sender_name, amount
                    ),
                    details: format!(
                        "LÃ­der {} transferiu {} moedas do tesouro pÃºblico para {} no dia {}.",
                        sender_name, amount, recipient_name, self.day
                    ),
                    known_by,
                };

                self.secrets.push(secret);

                for &wit_id in &eyewitnesses {
                    self.add_memory(
                        wit_id,
                        MemoryKind::Offense,
                        format!(
                            "Presenciei desvio de dinheiro pÃºblico por {}.",
                            sender_name
                        ),
                        vec!["corrupcao".to_string(), "crime".to_string()],
                        15,
                        vec![sender_id],
                    )?;
                    let wit_ent = self.find_agent_entity(wit_id)?;
                    let mut entry = self.world.entity_mut(wit_ent);
                    if let Some(mut state) = entry.get_mut::<StateComponent>() {
                        state.0.stress = (state.0.stress + 20).clamp(0, 100);
                    }
                }
            }
        } else {
            // Failed transfer due to lack of funds
            self.apply_relation_delta(
                recipient_id,
                sender_id,
                &RelationDelta {
                    resentment: 20,
                    friendship: -10,
                    trust: -20,
                    ..Default::default()
                },
            )?;
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: sender_id,
                target: Some(recipient_id),
                kind: EventKind::SocialBond,
                summary: format!(
                    "{} prometeu transferir recursos para {}, mas falhou por falta de fundos.",
                    sender_name, recipient_name
                ),
                impact_tags: vec!["social".to_string(), "falha".to_string()],
            });
        }

        if success {
            let mut fulfilled = Vec::new();
            for promise in &self.promises {
                if promise.promiser_id == sender_id && promise.promisee_id == recipient_id {
                    if let PromiseCondition::DeliverResource {
                        ref resource_id,
                        amount,
                    } = promise.condition
                    {
                        if resource_id == &transfer.resource_id && transfer.amount >= amount {
                            fulfilled.push(promise.id);
                        }
                    }
                }
            }
            for pid in fulfilled {
                self.fulfill_promise(pid)?;
            }
        }

        Ok(())
    }

    pub(super) fn execute_dialogue_secret_reveal(
        &mut self,
        sender_id: u64,
        reveal: &crate::agent_mind::RevealedSecret,
    ) -> Result<()> {
        let recipient_id = reveal.recipient_id;
        let secret_id = reveal.secret_id;

        let secret_idx = self.secrets.iter().position(|s| s.id == secret_id);
        let Some(idx) = secret_idx else {
            return Ok(());
        };

        if !self.secrets[idx].known_by.contains(&sender_id) {
            return Ok(());
        }

        let sender_name = self.agent_name(sender_id)?;
        let recipient_name = self.agent_name(recipient_id)?;

        if !self.secrets[idx].known_by.contains(&recipient_id) {
            self.secrets[idx].known_by.push(recipient_id);
        }

        self.add_memory(
            recipient_id,
            MemoryKind::Fact,
            format!("Recebi informaÃ§Ã£o privilegiada de {}.", sender_name),
            vec!["segredo".to_string(), "informacao".to_string()],
            20,
            vec![sender_id],
        )?;

        if self.secrets[idx].kind == SecretKind::CrimeCulprit {
            let crime_case_id = self.secrets[idx].target_id;
            let culprit_id = self.secrets[idx].details.parse::<u64>().unwrap_or(0);

            let role = {
                if let Ok(ent) = self.find_agent_entity(recipient_id) {
                    self.world
                        .entity(ent)
                        .get::<AgentCore>()
                        .unwrap()
                        .role_id
                        .clone()
                } else {
                    "".to_string()
                }
            };

            if role == "guarda" && culprit_id != 0 {
                if let Some(case) = self.crime_cases.iter_mut().find(|c| c.id == crime_case_id) {
                    case.suspect_id = Some(culprit_id);
                    case.status = CrimeCaseStatus::Proven;
                    case.confidence = 100;
                }
            }
        }

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: sender_id,
            target: Some(recipient_id),
            kind: EventKind::SocialBond,
            summary: format!(
                "{} revelou informaÃ§Ã£o privilegiada sobre '{}' para {}.",
                sender_name, self.secrets[idx].summary, recipient_name
            ),
            impact_tags: vec!["social".to_string(), "segredo".to_string()],
        });

        // Break KeepSecret promises if revealed to a third party
        let mut broken_promise_ids = Vec::new();
        for promise in &self.promises {
            if promise.promiser_id == sender_id {
                if let PromiseCondition::KeepSecret { secret_id: sid } = promise.condition {
                    if sid == secret_id && recipient_id != promise.promisee_id {
                        broken_promise_ids.push(promise.id);
                    }
                }
            }
        }
        for pid in broken_promise_ids {
            self.break_promise(pid)?;
        }

        // Release conditional escrow accounts
        let mut released_escrows = Vec::new();
        self.active_escrows.retain(|escrow| {
            if escrow.condition_secret_id == secret_id
                && escrow.target_agent_id == sender_id
                && escrow.depositor_id == recipient_id
            {
                released_escrows.push(escrow.clone());
                false
            } else {
                true
            }
        });

        for escrow in released_escrows {
            if let Ok(rec_ent) = self.find_agent_entity(escrow.target_agent_id) {
                let mut rec_entry = self.world.entity_mut(rec_ent);
                if let Some(mut inv) = rec_entry.get_mut::<InventoryComponent>() {
                    if let Some(stack) = inv
                        .0
                        .iter_mut()
                        .find(|s| s.resource_id == escrow.resource_id)
                    {
                        stack.amount += escrow.amount;
                    } else {
                        inv.0.push(ResourceStack {
                            resource_id: escrow.resource_id.clone(),
                            amount: escrow.amount,
                        });
                    }
                }
            }

            let depositor_name = self.agent_name(escrow.depositor_id)?;
            let target_name = self.agent_name(escrow.target_agent_id)?;

            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: escrow.target_agent_id,
                target: Some(escrow.depositor_id),
                kind: EventKind::SocialBond,
                summary: format!(
                    "CustÃ³dia liberada: {} recebeu {} {} de {} apÃ³s revelar o segredo {}.",
                    target_name,
                    escrow.amount,
                    escrow.resource_id,
                    depositor_name,
                    escrow.condition_secret_id
                ),
                impact_tags: vec![
                    "social".to_string(),
                    "escrow_liberado".to_string(),
                    escrow.resource_id.clone(),
                ],
            });
        }

        Ok(())
    }

    pub(super) fn execute_dialogue_make_promise(
        &mut self,
        speaker_id: u64,
        promise: &crate::agent_mind::ProposedPromise,
    ) -> Result<()> {
        let speaker_name = self.agent_name(speaker_id)?;
        let recipient_name = self.agent_name(promise.recipient_id)?;
        if self.has_equivalent_active_promise(speaker_id, promise.recipient_id, &promise.condition)
        {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: speaker_id,
                target: Some(promise.recipient_id),
                kind: EventKind::SocialBond,
                summary: format!(
                    "{} tenta repetir a mesma promessa para {}, mas ela ja esta ativa.",
                    speaker_name, recipient_name
                ),
                impact_tags: vec!["social".to_string(), "promessa_duplicada".to_string()],
            });
            return Ok(());
        }
        if self.has_recent_broken_promise_block(speaker_id, promise.recipient_id, 180) {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: speaker_id,
                target: Some(promise.recipient_id),
                kind: EventKind::SocialBond,
                summary: format!(
                    "{} tenta prometer algo a {}, mas a quebra recente de promessa ainda pesa.",
                    speaker_name, recipient_name
                ),
                impact_tags: vec!["social".to_string(), "promessa_bloqueada".to_string()],
            });
            return Ok(());
        }
        let backing_note = if let PromiseCondition::DeliverResource {
            resource_id,
            amount,
        } = &promise.condition
        {
            let Some(note) = self.deliver_promise_backing_note(
                speaker_id,
                promise.recipient_id,
                resource_id,
                *amount,
            ) else {
                self.add_memory(
                    promise.recipient_id,
                    MemoryKind::Impression,
                    format!(
                        "{} tentou prometer {} x{}, mas sem lastro material confiavel.",
                        speaker_name,
                        self.resource_display_name(resource_id),
                        amount
                    ),
                    vec![
                        "social".to_string(),
                        "promessa_fragil".to_string(),
                        resource_id.clone(),
                    ],
                    10,
                    vec![speaker_id],
                )?;
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: speaker_id,
                    target: Some(promise.recipient_id),
                    kind: EventKind::SocialBond,
                    summary: format!(
                        "{} tentou prometer {} x{} para {}, mas sem estoque, escrow, task ou caixa suficientes.",
                        speaker_name,
                        self.resource_display_name(resource_id),
                        amount,
                        recipient_name
                    ),
                    impact_tags: vec![
                        "social".to_string(),
                        "promessa_fragil".to_string(),
                        resource_id.clone(),
                    ],
                });
                return Ok(());
            };
            Some(note)
        } else {
            None
        };

        let next_id = self.promises.iter().map(|p| p.id).max().unwrap_or(0) + 1;
        let active = ActivePromise {
            id: next_id,
            promiser_id: speaker_id,
            promisee_id: promise.recipient_id,
            condition: promise.condition.clone(),
            deadline_tick: (self.total_ticks as u32) + promise.duration_ticks,
            created_at_tick: self.total_ticks as u32,
        };
        self.promises.push(active);
        let promise_summary = self.describe_promise_condition(&promise.condition);
        let memory_summary = if let Some(note) = &backing_note {
            format!("{} me prometeu {} ({note}).", speaker_name, promise_summary)
        } else {
            format!("{} me prometeu {}.", speaker_name, promise_summary)
        };

        self.add_memory(
            promise.recipient_id,
            MemoryKind::Promise,
            memory_summary,
            vec![
                "social".to_string(),
                "promessa".to_string(),
                "promessa_ativa".to_string(),
            ],
            10,
            vec![speaker_id],
        )?;

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: speaker_id,
            target: Some(promise.recipient_id),
            kind: EventKind::SocialBond,
            summary: if let Some(note) = backing_note {
                format!(
                    "{} fez promessa para {}: {} ({note}).",
                    speaker_name, recipient_name, promise_summary
                )
            } else {
                format!(
                    "{} fez promessa para {}: {}.",
                    speaker_name, recipient_name, promise_summary
                )
            },
            impact_tags: vec!["social".to_string(), "promessa".to_string()],
        });
        Ok(())
    }

    fn has_equivalent_active_promise(
        &self,
        promiser_id: u64,
        promisee_id: u64,
        condition: &PromiseCondition,
    ) -> bool {
        self.promises.iter().any(|promise| {
            promise.promiser_id == promiser_id
                && promise.promisee_id == promisee_id
                && &promise.condition == condition
        })
    }

    fn has_recent_broken_promise_block(
        &self,
        promiser_id: u64,
        promisee_id: u64,
        within_ticks: u64,
    ) -> bool {
        self.has_recent_event(within_ticks, |event| {
            event.kind == EventKind::SocialBond
                && event.actor == promiser_id
                && event.target == Some(promisee_id)
                && event.summary.contains("quebrou a promessa")
        })
    }

    fn describe_promise_condition(&self, condition: &PromiseCondition) -> String {
        match condition {
            PromiseCondition::DeliverResource {
                resource_id,
                amount,
            } => format!(
                "entregar {} x{}",
                self.resource_display_name(resource_id),
                amount
            ),
            PromiseCondition::VoteForPolicy { domain, value } => {
                format!("votar em {domain} -> {value}")
            }
            PromiseCondition::KeepSecret { secret_id } => {
                format!("guardar o segredo #{secret_id}")
            }
        }
    }

    fn deliver_promise_backing_note(
        &mut self,
        promiser_id: u64,
        promisee_id: u64,
        resource_id: &str,
        amount: i32,
    ) -> Option<String> {
        let amount = amount.max(1);
        let promiser_entity = self.find_agent_entity(promiser_id).ok()?;
        let carried_amount = self
            .world
            .entity(promiser_entity)
            .get::<InventoryComponent>()
            .map(|inventory| Self::total_resource_amount(&inventory.0, resource_id))
            .unwrap_or(0);
        if carried_amount >= amount {
            return Some("lastro em estoque pessoal".to_string());
        }

        let household_id = self.household_id_for_agent(promiser_id)?;
        if let Some(household) = self.household_by_id(household_id) {
            let household_stock = Self::total_resource_amount(&household.pantry, resource_id)
                + Self::total_resource_amount(&household.reserved_food, resource_id);
            if household_stock >= amount {
                return Some("lastro na despensa do lar".to_string());
            }
            if self
                .resource_def(resource_id)
                .map(|resource| resource.can_buy_external)
                .unwrap_or(false)
            {
                let unit_price = self
                    .market_quote(resource_id)
                    .map(|quote| quote.buy_price)
                    .unwrap_or_else(|| self.base_price(resource_id) * 2);
                if household.treasury >= unit_price * amount {
                    return Some("lastro em caixa para compra externa".to_string());
                }
            }
        }

        let establishment_stock = self
            .establishments
            .iter()
            .filter(|establishment| establishment.owner_household_ids.contains(&household_id))
            .map(|establishment| Self::total_resource_amount(&establishment.stock, resource_id))
            .sum::<i32>();
        if establishment_stock >= amount {
            return Some("lastro em estoque do estabelecimento".to_string());
        }

        if self.active_escrows.iter().any(|escrow| {
            escrow.depositor_id == promiser_id
                && escrow.target_agent_id == promisee_id
                && escrow.resource_id == resource_id
                && escrow.amount >= amount
        }) {
            return Some("lastro em custodia real".to_string());
        }

        if self.economic_tasks.iter().any(|task| {
            task.actor_household_id == household_id
                && task.resource_id.as_deref() == Some(resource_id)
                && matches!(
                    task.kind,
                    EconomicTaskKind::Comprar
                        | EconomicTaskKind::Transportar
                        | EconomicTaskKind::Produzir
                )
                && task.phase != EconomicTaskPhase::Completed
                && task.phase != EconomicTaskPhase::Failed
                && task.amount >= amount
        }) {
            return Some("lastro em task economica ativa".to_string());
        }

        None
    }

    pub(super) fn execute_dialogue_spread_rumor(
        &mut self,
        sender_id: u64,
        listener_id: u64,
        proposed: &crate::agent_mind::ProposedRumor,
    ) -> Result<()> {
        let sender_name = self.agent_name(sender_id)?;
        let listener_name = self.agent_name(listener_id)?;
        let target_name = self.agent_name(proposed.target_agent_id)?;
        let claim = proposed
            .claim
            .clone()
            .unwrap_or_else(|| proposed.topic.clone());
        let topic = proposed.topic.trim().to_lowercase();

        let rumor_idx = self
            .rumors
            .iter()
            .position(|r| r.target_agent_id == proposed.target_agent_id && r.topic == topic);

        let rumor = if let Some(idx) = rumor_idx {
            let snapshot = self.rumors[idx].clone();
            let distortion_delta =
                self.deterministic_distortion_for_spread(&snapshot, sender_id, listener_id);
            let distorted_claim = self.distorted_claim(&snapshot, distortion_delta);
            let r = &mut self.rumors[idx];
            if !r.known_by.contains(&sender_id) {
                r.known_by.push(sender_id);
            }
            if !r.known_by.contains(&listener_id) {
                r.known_by.push(listener_id);
            }
            if !r.current_carrier_ids.contains(&listener_id) {
                r.current_carrier_ids.push(listener_id);
            }
            r.claim = distorted_claim;
            r.distortion = (r.distortion + distortion_delta).clamp(0, 100);
            r.last_spread_tick = self.total_ticks;
            r.spread_count = r.spread_count.saturating_add(1);
            r.clone()
        } else {
            let rumor_id = self.next_rumor_id;
            self.next_rumor_id += 1;
            let new_rumor = Rumor {
                id: rumor_id,
                source_agent_id: sender_id,
                current_carrier_ids: vec![sender_id, listener_id],
                target_agent_id: proposed.target_agent_id,
                about_agent_id: Some(proposed.target_agent_id),
                about_household_id: self.household_id_for_agent(proposed.target_agent_id),
                about_policy_act_id: None,
                about_crime_case_id: None,
                claim: claim.clone(),
                topic: topic.clone(),
                truth_score: if proposed.is_true { 85 } else { 15 },
                distortion: if proposed.is_true { 8 } else { 35 },
                credibility_seed: if proposed.is_true { 65 } else { 35 },
                known_by: vec![sender_id, listener_id],
                origin_day: self.day,
                origin_tick: self.tick_of_day,
                last_spread_tick: self.total_ticks,
                spread_count: 1,
                is_slander: !proposed.is_true,
                is_confirmed: false,
                is_disproven: false,
            };
            self.rumors.push(new_rumor.clone());
            new_rumor
        };

        let belief = self.base_rumor_belief(&rumor, sender_id, listener_id);
        let skepticism = (100 - belief + rumor.distortion / 2).clamp(0, 100);
        let belief_record =
            self.upsert_rumor_belief(listener_id, rumor.id, Some(sender_id), belief, skepticism)?;
        let _speaker_record =
            self.upsert_rumor_belief(sender_id, rumor.id, Some(sender_id), 15, 5)?;
        let memory_kind = if belief_record.belief >= 60 {
            MemoryKind::Fact
        } else {
            MemoryKind::Impression
        };
        self.add_memory(
            listener_id,
            memory_kind,
            format!(
                "Ouvi rumor de {} sobre {}: {} (crenca {}, distorcao {}).",
                sender_name, target_name, rumor.claim, belief_record.belief, rumor.distortion
            ),
            vec!["boato".to_string(), "social".to_string(), topic.clone()],
            5,
            vec![sender_id, proposed.target_agent_id],
        )?;

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: sender_id,
            target: Some(listener_id),
            kind: EventKind::ConversationTurn,
            summary: format!(
                "{} espalhou rumor para {} sobre {}: {} (crenca {}, distorcao {}).",
                sender_name,
                listener_name,
                target_name,
                rumor.claim,
                belief_record.belief,
                rumor.distortion
            ),
            impact_tags: vec!["social".to_string(), "boato".to_string(), topic.clone()],
        });

        if rumor.is_slander {
            self.apply_relation_delta(
                proposed.target_agent_id,
                sender_id,
                &RelationDelta {
                    resentment: 4,
                    trust: -3,
                    reputation: -2,
                    ..Default::default()
                },
            )?;
        }

        let corruption_rumor = topic.contains("corrup")
            || topic.contains("desvio")
            || rumor.claim.to_lowercase().contains("corrup")
            || rumor.claim.to_lowercase().contains("desvio");
        if corruption_rumor && belief_record.belief >= 50 {
            let mut delta = InstitutionalPerception::zero_delta();
            delta.leader_legitimacy = -4;
            delta.guard_trust = -2;
            delta.perceived_corruption = 6;
            delta.perceived_fairness = -3;
            self.adjust_institutional_perception(
                listener_id,
                delta,
                format!("acreditou rumor de corrupcao #{}", rumor.id),
            )?;
        }

        if self.has_justice_authority(listener_id)? && rumor.is_slander {
            if let Ok(ent) = self.find_agent_entity(listener_id) {
                let mut entry = self.world.entity_mut(ent);
                if let Some(mut queue) = entry.get_mut::<TaskQueueComponent>() {
                    queue.0.push_back(SimplifiedTask {
                        kind: IntentKind::Investigar,
                        target_semantic: None,
                        target_agent: Some(proposed.target_agent_id),
                        social_move: None,
                    });
                }
            }
        }

        Ok(())
    }

    pub(super) fn execute_dialogue_share_story(
        &mut self,
        sender_id: u64,
        listener_id: u64,
        proposed: &crate::agent_mind::ProposedStoryShare,
    ) -> Result<()> {
        let sender_name = self.agent_name(sender_id)?;
        let listener_name = self.agent_name(listener_id)?;
        let story_id = if let Some(existing_id) = proposed.story_id {
            if self
                .cultural_stories
                .iter()
                .any(|story| story.id == existing_id)
            {
                existing_id
            } else {
                self.create_cultural_story_from_share(sender_id, listener_id, proposed)?
            }
        } else if let Some(existing_id) = self.match_cultural_story_by_title_or_core(proposed) {
            existing_id
        } else {
            self.create_cultural_story_from_share(sender_id, listener_id, proposed)?
        };

        let story_snapshot = self
            .cultural_stories
            .iter()
            .find(|story| story.id == story_id)
            .cloned()
            .ok_or_else(|| anyhow!("story {story_id} not found after share"))?;
        let distortion_delta =
            self.deterministic_story_distortion(&story_snapshot, sender_id, listener_id);
        let version_id = self
            .story_versions
            .iter()
            .map(|version| version.id)
            .max()
            .unwrap_or(0)
            + 1;
        let generation = self.agent_generation(sender_id);
        self.story_versions.push(StoryVersion {
            id: version_id,
            story_id,
            short_version: self.distorted_story_version(&proposed.version, distortion_delta),
            author_agent_id: Some(sender_id),
            transmitter_agent_id: Some(sender_id),
            generation,
            tone: proposed.tone.clone().unwrap_or_else(|| "oral".to_string()),
            distortion: (story_snapshot.distortion + distortion_delta).clamp(0, 100),
            cultural_tags: proposed.tags.clone(),
            created_day: self.day,
            created_tick: self.tick_of_day,
        });

        if let Some(story) = self
            .cultural_stories
            .iter_mut()
            .find(|story| story.id == story_id)
        {
            story.cultural_strength = (story.cultural_strength + 4).clamp(0, 100);
            story.distortion = (story.distortion + distortion_delta).clamp(0, 100);
            story.last_told_tick = self.total_ticks;
            story.tell_count = story.tell_count.saturating_add(1);
            if story.tell_count >= 5 && story.stability >= 45 && story.distortion <= 45 {
                story.status = StoryStatus::Estavel;
            }
            if story.tell_count >= 12 && story.cultural_strength >= 75 && story.distortion <= 35 {
                story.status = StoryStatus::Canonizada;
            }
        }

        let relation = self.relation_between(listener_id, sender_id);
        let belief_delta = (30 + relation.trust / 4 + story_snapshot.cultural_strength / 5
            - story_snapshot.distortion / 4)
            .clamp(5, 80);
        let attachment_delta = (18 + story_snapshot.cultural_strength / 8).clamp(5, 60);
        let moral = proposed
            .moral
            .clone()
            .unwrap_or_else(|| story_snapshot.moral.clone());
        let belief = self.upsert_story_belief(
            listener_id,
            story_id,
            Some(sender_id),
            belief_delta,
            attachment_delta,
            moral.clone(),
        )?;
        self.upsert_story_belief(
            sender_id,
            story_id,
            Some(sender_id),
            8,
            5,
            story_snapshot.moral.clone(),
        )?;

        self.apply_story_psychological_effect(
            listener_id,
            &story_snapshot,
            belief.emotional_attachment,
        )?;
        self.add_memory(
            listener_id,
            MemoryKind::Impression,
            format!(
                "Ouvi de {} a historia '{}': {}",
                sender_name, story_snapshot.title, proposed.version
            ),
            vec![
                "historia_cultural".to_string(),
                story_snapshot.theme.clone(),
                format!("{:?}", story_snapshot.origin_kind).to_lowercase(),
            ],
            (belief.emotional_attachment / 6).clamp(3, 18),
            story_snapshot.cited_agent_ids.clone(),
        )?;

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: sender_id,
            target: Some(listener_id),
            kind: EventKind::CulturalStory,
            summary: format!(
                "{} contou a {} a historia '{}': crenca={} apego={} distorcao={}",
                sender_name,
                listener_name,
                story_snapshot.title,
                belief.belief,
                belief.emotional_attachment,
                story_snapshot.distortion + distortion_delta
            ),
            impact_tags: vec![
                "cultura".to_string(),
                "historia".to_string(),
                story_snapshot.theme,
            ],
        });

        Ok(())
    }

    fn create_cultural_story_from_share(
        &mut self,
        sender_id: u64,
        listener_id: u64,
        proposed: &crate::agent_mind::ProposedStoryShare,
    ) -> Result<CulturalStoryId> {
        let story_id = self.next_cultural_story_id;
        self.next_cultural_story_id += 1;
        let title = proposed
            .title
            .clone()
            .unwrap_or_else(|| self.story_title_from_version(&proposed.version));
        let origin_kind = proposed
            .kind
            .as_deref()
            .map(Self::parse_cultural_story_kind)
            .unwrap_or(CulturalStoryKind::Lenda);
        let position = self.debug_agent_position(sender_id).ok();
        let associated_building_id =
            position.and_then(|pos| self.tile_at(pos).and_then(|tile| tile.building_id));
        let story = CulturalStory {
            id: story_id,
            title,
            narrative_core: proposed.version.clone(),
            origin_kind,
            theme: self.story_theme_from_kind(origin_kind).to_string(),
            moral: proposed
                .moral
                .clone()
                .unwrap_or_else(|| self.story_moral_from_kind(origin_kind).to_string()),
            cited_agent_ids: vec![sender_id, listener_id],
            associated_building_id,
            associated_territory_id: None,
            source_event_summaries: Vec::new(),
            origin_generation: self.agent_generation(sender_id),
            cultural_strength: 18,
            stability: 10,
            distortion: 12,
            status: StoryStatus::Emergente,
            created_day: self.day,
            last_told_tick: self.total_ticks,
            tell_count: 0,
        };
        self.cultural_stories.push(story);
        Ok(story_id)
    }

    fn match_cultural_story_by_title_or_core(
        &self,
        proposed: &crate::agent_mind::ProposedStoryShare,
    ) -> Option<CulturalStoryId> {
        let title = proposed.title.as_deref().map(Self::normalize_story_key);
        let version_key = Self::normalize_story_key(&proposed.version);
        self.cultural_stories.iter().find_map(|story| {
            let title_match = title
                .as_ref()
                .is_some_and(|title| *title == Self::normalize_story_key(&story.title));
            let core_match = Self::normalize_story_key(&story.narrative_core)
                .chars()
                .take(48)
                .collect::<String>()
                == version_key.chars().take(48).collect::<String>();
            (title_match || core_match).then_some(story.id)
        })
    }

    fn normalize_story_key(raw: &str) -> String {
        raw.to_lowercase()
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .take(80)
            .collect()
    }

    fn story_title_from_version(&self, version: &str) -> String {
        let mut title = version
            .split(['.', ',', ';', ':'])
            .next()
            .unwrap_or(version)
            .trim()
            .chars()
            .take(48)
            .collect::<String>();
        if title.is_empty() {
            title = "Historia sem nome".to_string();
        }
        title
    }

    fn parse_cultural_story_kind(raw: &str) -> CulturalStoryKind {
        match raw.to_lowercase().replace([' ', '_', '-'], "").as_str() {
            "historiafamiliar" | "familia" => CulturalStoryKind::HistoriaFamiliar,
            "cantodeguerra" | "guerra" => CulturalStoryKind::CantoDeGuerra,
            "martirio" | "martir" => CulturalStoryKind::Martirio,
            "milagre" => CulturalStoryKind::Milagre,
            "assombracao" | "fantasma" | "medo" => CulturalStoryKind::Assombracao,
            "fundacao" | "origem" => CulturalStoryKind::Fundacao,
            "traicao" | "traidor" => CulturalStoryKind::Traicao,
            "heroismo" | "heroi" => CulturalStoryKind::Heroismo,
            "advertenciamoral" | "aviso" => CulturalStoryKind::AdvertenciaMoral,
            _ => CulturalStoryKind::Lenda,
        }
    }

    fn story_theme_from_kind(&self, kind: CulturalStoryKind) -> &'static str {
        match kind {
            CulturalStoryKind::HistoriaFamiliar => "familia",
            CulturalStoryKind::CantoDeGuerra => "guerra",
            CulturalStoryKind::Martirio => "martirio",
            CulturalStoryKind::Milagre => "esperanca",
            CulturalStoryKind::Assombracao => "medo",
            CulturalStoryKind::Fundacao => "fundacao",
            CulturalStoryKind::Traicao => "traicao",
            CulturalStoryKind::Heroismo => "heroismo",
            CulturalStoryKind::AdvertenciaMoral => "moral",
            CulturalStoryKind::Lenda => "lenda",
        }
    }

    fn story_moral_from_kind(&self, kind: CulturalStoryKind) -> &'static str {
        match kind {
            CulturalStoryKind::HistoriaFamiliar => "a familia preserva o que a vila esquece",
            CulturalStoryKind::CantoDeGuerra => "a coragem cobra um preco",
            CulturalStoryKind::Martirio => "a injustica contra um vira memoria de todos",
            CulturalStoryKind::Milagre => "a esperanca nasce quando a escassez aperta",
            CulturalStoryKind::Assombracao => "certos lugares guardam medo",
            CulturalStoryKind::Fundacao => "toda construcao tem uma divida com seus fundadores",
            CulturalStoryKind::Traicao => "confianca quebrada raramente volta inteira",
            CulturalStoryKind::Heroismo => "um ato de coragem pode salvar muitos",
            CulturalStoryKind::AdvertenciaMoral => "quem ignora sinais repete quedas antigas",
            CulturalStoryKind::Lenda => "historias antigas moldam escolhas novas",
        }
    }

    fn deterministic_story_distortion(
        &mut self,
        story: &CulturalStory,
        sender_id: u64,
        listener_id: u64,
    ) -> i32 {
        let relation = self.relation_between(sender_id, listener_id);
        let distrust = (-relation.trust).max(0) / 10 + relation.resentment.max(0) / 12;
        let repetition = (story.tell_count as i32 / 3).clamp(0, 8);
        let chaos = self.agent_chaos_pressure(sender_id).unwrap_or(0) as i32 / 25;
        (1 + distrust + repetition + chaos - story.stability / 30).clamp(0, 14)
    }

    fn distorted_story_version(&self, version: &str, distortion_delta: i32) -> String {
        if distortion_delta <= 3 {
            version.to_string()
        } else if distortion_delta <= 8 {
            format!("Dizem que {version}")
        } else {
            format!("A historia cresceu assim: {version}")
        }
    }

    fn agent_generation(&mut self, agent_id: u64) -> u32 {
        let Ok(entity) = self.find_agent_entity(agent_id) else {
            return 0;
        };
        self.world
            .entity(entity)
            .get::<LineageComponent>()
            .map(|lineage| if lineage.parents.is_empty() { 0 } else { 1 })
            .unwrap_or(0)
    }

    fn inherit_parental_stories(
        &mut self,
        child_id: u64,
        mother_id: u64,
        father_id: u64,
        strength_percent: i32,
    ) -> Result<()> {
        let mut inherited: HashMap<CulturalStoryId, StoryBelief> = HashMap::new();
        for parent_id in [mother_id, father_id] {
            for belief in self.story_beliefs(parent_id) {
                if belief.belief < 25 && belief.emotional_attachment < 25 {
                    continue;
                }
                inherited
                    .entry(belief.story_id)
                    .and_modify(|existing| {
                        existing.belief = existing.belief.max(belief.belief);
                        existing.emotional_attachment = existing
                            .emotional_attachment
                            .max(belief.emotional_attachment);
                    })
                    .or_insert(belief);
            }
        }
        for (_, belief) in inherited.into_iter().take(6) {
            let belief_delta = (belief.belief * strength_percent / 100).clamp(5, 60);
            let attachment_delta =
                (belief.emotional_attachment * strength_percent / 100).clamp(5, 55);
            self.upsert_story_belief(
                child_id,
                belief.story_id,
                belief.heard_from.or(Some(mother_id)),
                belief_delta,
                attachment_delta,
                belief.moral_interpretation,
            )?;
        }
        Ok(())
    }

    fn apply_story_psychological_effect(
        &mut self,
        agent_id: u64,
        story: &CulturalStory,
        attachment: i32,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entry = self.world.entity_mut(entity);
        if let Some(mut psychology) = entry.get_mut::<PsychologicalStateComponent>() {
            let delta = (attachment / 20).clamp(1, 5);
            match story.origin_kind {
                CulturalStoryKind::Martirio => {
                    psychology.0.grief = (psychology.0.grief + delta).clamp(0, 100);
                    psychology.0.pride = (psychology.0.pride + delta).clamp(0, 100);
                    psychology.0.anger = (psychology.0.anger + delta).clamp(0, 100);
                }
                CulturalStoryKind::Assombracao => {
                    psychology.0.fear = (psychology.0.fear + delta).clamp(0, 100);
                }
                CulturalStoryKind::Heroismo | CulturalStoryKind::CantoDeGuerra => {
                    psychology.0.pride = (psychology.0.pride + delta).clamp(0, 100);
                    psychology.0.hope = (psychology.0.hope + delta / 2).clamp(0, 100);
                }
                CulturalStoryKind::Traicao => {
                    psychology.0.fear = (psychology.0.fear + delta / 2).clamp(0, 100);
                    psychology.0.anger = (psychology.0.anger + delta).clamp(0, 100);
                }
                _ => {
                    psychology.0.hope = (psychology.0.hope + delta / 2).clamp(0, 100);
                }
            }
        }
        Ok(())
    }

    pub(super) fn execute_dialogue_escrow_deposit(
        &mut self,
        sender_id: u64,
        escrow: &crate::agent_mind::ProposedEscrow,
    ) -> Result<()> {
        let recipient_id = escrow.target_agent_id;
        let amount = escrow.amount.max(0);
        if amount == 0 {
            return Ok(());
        }

        let mut success = false;
        if let Ok(sender_ent) = self.find_agent_entity(sender_id) {
            let mut sender_entry = self.world.entity_mut(sender_ent);
            if let Some(mut inv) = sender_entry.get_mut::<InventoryComponent>() {
                if let Some(stack) = inv
                    .0
                    .iter_mut()
                    .find(|s| s.resource_id == escrow.resource_id)
                {
                    if stack.amount >= amount {
                        stack.amount -= amount;
                        success = true;
                    }
                }
            }
        }

        if success {
            let escrow_id = self.active_escrows.iter().map(|e| e.id).max().unwrap_or(0) + 1;
            let account = EscrowAccount {
                id: escrow_id,
                depositor_id: sender_id,
                target_agent_id: recipient_id,
                resource_id: escrow.resource_id.clone(),
                amount,
                condition_secret_id: escrow.condition_secret_id,
            };
            self.active_escrows.push(account);

            let sender_name = self.agent_name(sender_id)?;
            let recipient_name = self.agent_name(recipient_id)?;

            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: sender_id,
                target: Some(recipient_id),
                kind: EventKind::SocialBond,
                summary: format!(
                    "{} depositou {} {} em custÃ³dia para {} aguardando revelaÃ§Ã£o do segredo {}.",
                    sender_name,
                    amount,
                    escrow.resource_id,
                    recipient_name,
                    escrow.condition_secret_id
                ),
                impact_tags: vec![
                    "social".to_string(),
                    "escrow".to_string(),
                    escrow.resource_id.clone(),
                ],
            });
        }

        Ok(())
    }

    pub fn fulfill_promise(&mut self, promise_id: u64) -> Result<()> {
        let pos = self.promises.iter().position(|p| p.id == promise_id);
        let Some(idx) = pos else {
            return Ok(());
        };
        let promise = self.promises.remove(idx);

        let promiser_name = self.agent_name(promise.promiser_id)?;
        let promisee_name = self.agent_name(promise.promisee_id)?;

        self.apply_relation_delta(
            promise.promisee_id,
            promise.promiser_id,
            &RelationDelta {
                trust: 10,
                friendship: 5,
                resentment: -5,
                ..Default::default()
            },
        )?;

        self.add_memory(
            promise.promisee_id,
            MemoryKind::Success,
            format!("{} cumpriu sua promessa de forma honrada.", promiser_name),
            vec!["promessa".to_string(), "sucesso".to_string()],
            15,
            vec![promise.promiser_id],
        )?;

        self.add_memory(
            promise.promiser_id,
            MemoryKind::Success,
            format!("Cumpri minha promessa para {}.", promisee_name),
            vec!["promessa".to_string(), "sucesso".to_string()],
            10,
            vec![promise.promisee_id],
        )?;

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: promise.promiser_id,
            target: Some(promise.promisee_id),
            kind: EventKind::SocialBond,
            summary: format!(
                "{} cumpriu a promessa feita a {}.",
                promiser_name, promisee_name
            ),
            impact_tags: vec![
                "social".to_string(),
                "promessa".to_string(),
                "sucesso".to_string(),
            ],
        });

        Ok(())
    }

    pub fn break_promise(&mut self, promise_id: u64) -> Result<()> {
        let pos = self.promises.iter().position(|p| p.id == promise_id);
        let Some(idx) = pos else {
            return Ok(());
        };
        let promise = self.promises.remove(idx);

        let promiser_name = self.agent_name(promise.promiser_id)?;
        let promisee_name = self.agent_name(promise.promisee_id)?;

        self.apply_relation_delta(
            promise.promisee_id,
            promise.promiser_id,
            &RelationDelta {
                trust: -25,
                friendship: -15,
                resentment: 20,
                ..Default::default()
            },
        )?;

        self.apply_trauma_trait(promise.promiser_id, "traidor")?;
        self.mark_revenge_target(
            promise.promisee_id,
            promise.promiser_id,
            18,
            format!("promessa quebrada por {}", promiser_name),
        )?;

        self.add_memory(
            promise.promisee_id,
            MemoryKind::Failure,
            format!("{} quebrou a promessa que me fez.", promiser_name),
            vec![
                "promessa".to_string(),
                "traicao".to_string(),
                "ressentimento".to_string(),
            ],
            25,
            vec![promise.promiser_id],
        )?;

        let secret_id = {
            let id = self.next_secret_id;
            self.next_secret_id += 1;
            id
        };
        let secret = Secret {
            id: secret_id,
            kind: SecretKind::BrokenPromise,
            target_id: promise.promiser_id,
            summary: format!("A traiÃ§Ã£o de {} contra {}.", promiser_name, promisee_name),
            details: format!("Quebra de promessa contract_id {}", promise.id),
            known_by: vec![promise.promisee_id, promise.promiser_id],
        };
        self.secrets.push(secret);

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: promise.promiser_id,
            target: Some(promise.promisee_id),
            kind: EventKind::SocialBond,
            summary: format!(
                "{} quebrou a promessa feita a {}.",
                promiser_name, promisee_name
            ),
            impact_tags: vec![
                "social".to_string(),
                "promessa".to_string(),
                "traicao".to_string(),
            ],
        });

        Ok(())
    }

    pub fn check_active_promises(&mut self) -> Result<()> {
        let mut fulfilled = Vec::new();
        let mut broken = Vec::new();

        fn policy_domain_from_str(s: &str) -> Option<PolicyDomain> {
            match s.to_lowercase().as_str() {
                "imposto" | "tax" | "taxa_imposto" => Some(PolicyDomain::Tax),
                "justica" | "justice" => Some(PolicyDomain::Justice),
                "racionamento" | "rationing" => Some(PolicyDomain::Rationing),
                _ => None,
            }
        }

        for promise in &self.promises {
            if self.total_ticks > promise.deadline_tick as u64 {
                match promise.condition {
                    PromiseCondition::KeepSecret { .. } => {
                        fulfilled.push(promise.id);
                    }
                    _ => {
                        broken.push(promise.id);
                    }
                }
                continue;
            }

            if let PromiseCondition::VoteForPolicy {
                ref domain,
                ref value,
            } = promise.condition
            {
                let target_domain = policy_domain_from_str(domain);
                let mut found_vote = false;
                for issue in &self.political_issues {
                    let domain_match =
                        Some(issue.domain) == target_domain || issue.domain.as_str() == domain;
                    if domain_match && &issue.proposed_value == value {
                        if issue.supporter_ids.contains(&promise.promiser_id) {
                            found_vote = true;
                            break;
                        }
                    }
                }
                if found_vote {
                    fulfilled.push(promise.id);
                }
            }
        }

        for pid in fulfilled {
            self.fulfill_promise(pid)?;
        }
        for pid in broken {
            self.break_promise(pid)?;
        }

        Ok(())
    }

    pub(super) fn apply_conversation_effects(
        &mut self,
        speaker_id: u64,
        listener_id: u64,
        speech_act: &str,
        emotion: &str,
        risk_shift: i32,
        belief_updates: &[String],
    ) -> Result<()> {
        let speaker_name = self.agent_name(speaker_id)?;
        let listener_name = self.agent_name(listener_id)?;
        for agent_id in [speaker_id, listener_id] {
            let entity = self.find_agent_entity(agent_id)?;
            let mut entity_mut = self.world.entity_mut(entity);
            let mut state = entity_mut
                .get_mut::<StateComponent>()
                .ok_or_else(|| anyhow!("missing state component"))?;
            state.0.stress = (state.0.stress + risk_shift).clamp(0, 100);
            state.0.energy = (state.0.energy - 1).clamp(0, 100);
            if let Some(goal) = belief_updates.first() {
                if !state.0.active_goals.iter().any(|existing| existing == goal) {
                    state.0.active_goals.push(goal.clone());
                }
                if state.0.active_goals.len() > 4 {
                    state.0.active_goals.truncate(4);
                }
            }
        }
        self.set_thought(
            speaker_id,
            format!("Quero {} {}.", speech_act, listener_name),
        )?;
        self.set_thought(
            listener_id,
            format!("{speaker_name} fala comigo com emocao {emotion}."),
        )?;
        self.add_memory(
            speaker_id,
            MemoryKind::Reflection,
            format!("Eu disse a {}: {}", listener_name, speech_act),
            vec!["social".to_string(), "conversa".to_string()],
            6,
            vec![listener_id],
        )?;
        self.add_memory(
            listener_id,
            MemoryKind::Impression,
            format!("{speaker_name} falou comigo: {speech_act}"),
            vec!["social".to_string(), "conversa".to_string()],
            6,
            vec![speaker_id],
        )?;
        Ok(())
    }

    pub(super) fn end_conversation(
        &mut self,
        conversation_id: ConversationId,
        status: ConversationStatus,
        outcome: ConversationOutcome,
        reason: String,
    ) -> Result<()> {
        let (participants, summary) = {
            let conversation = self
                .conversation_state_mut(conversation_id)
                .ok_or_else(|| anyhow!("conversation {conversation_id} not found"))?;
            conversation.status = status.clone();
            conversation.outcome = outcome.clone();
            conversation.end_reason = Some(reason.clone());
            (conversation.participants, conversation.summary.clone())
        };

        let [agent_a, agent_b] = participants;

        // Refund any active escrows between the participants of this conversation
        let mut refunded_escrows = Vec::new();
        self.active_escrows.retain(|escrow| {
            let matches_participants = (escrow.depositor_id == agent_a
                && escrow.target_agent_id == agent_b)
                || (escrow.depositor_id == agent_b && escrow.target_agent_id == agent_a);
            if matches_participants {
                refunded_escrows.push(escrow.clone());
                false
            } else {
                true
            }
        });

        for escrow in refunded_escrows {
            if let Ok(dep_ent) = self.find_agent_entity(escrow.depositor_id) {
                let mut dep_entry = self.world.entity_mut(dep_ent);
                if let Some(mut inv) = dep_entry.get_mut::<InventoryComponent>() {
                    if let Some(stack) = inv
                        .0
                        .iter_mut()
                        .find(|s| s.resource_id == escrow.resource_id)
                    {
                        stack.amount += escrow.amount;
                    } else {
                        inv.0.push(ResourceStack {
                            resource_id: escrow.resource_id.clone(),
                            amount: escrow.amount,
                        });
                    }
                }
            }

            let depositor_name = self.agent_name(escrow.depositor_id)?;
            let target_name = self.agent_name(escrow.target_agent_id)?;

            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: escrow.depositor_id,
                target: Some(escrow.target_agent_id),
                kind: EventKind::SocialBond,
                summary: format!(
                    "CustÃ³dia reembolsada: {} recebeu de volta {} {} pois a conversa com {} terminou sem a revelaÃ§Ã£o do segredo.",
                    depositor_name, escrow.amount, escrow.resource_id, target_name
                ),
                impact_tags: vec!["social".to_string(), "escrow_reembolsado".to_string(), escrow.resource_id.clone()],
            });
        }

        for (agent_id, other_id) in [(agent_a, agent_b), (agent_b, agent_a)] {
            let other_name = self.agent_name(other_id)?;
            self.release_agent_from_conversation(agent_id, reason.clone())?;
            self.add_memory(
                agent_id,
                if matches!(outcome, ConversationOutcome::PhysicalConflict) {
                    MemoryKind::Offense
                } else {
                    MemoryKind::Impression
                },
                format!("Conversa com {} terminou: {}", other_name, summary),
                vec!["social".to_string(), "conversa".to_string()],
                14,
                vec![other_id],
            )?;
        }
        let agent_a_name = self.agent_name(agent_a)?;
        let agent_b_name = self.agent_name(agent_b)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_a,
            target: Some(agent_b),
            kind: EventKind::ConversationEnded,
            summary: format!(
                "Conversa entre {} e {} termina: {}.",
                agent_a_name, agent_b_name, reason
            ),
            impact_tags: vec![
                "social".to_string(),
                "conversa".to_string(),
                format!("outcome:{:?}", outcome).to_lowercase(),
            ],
        });
        Ok(())
    }

    pub(super) fn conversation_state(
        &self,
        conversation_id: ConversationId,
    ) -> Option<ConversationState> {
        self.conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .cloned()
    }

    pub(super) fn conversation_state_mut(
        &mut self,
        conversation_id: ConversationId,
    ) -> Option<&mut ConversationState> {
        self.conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
    }

    pub(super) fn agent_conversation_id(
        &mut self,
        agent_id: u64,
    ) -> Result<Option<ConversationId>> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?
            .active_conversation_id)
    }

    pub(super) fn agent_social_cooldown_until(&mut self, agent_id: u64) -> Result<u64> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?
            .social_cooldown_until)
    }

    pub(super) fn bind_agent_to_conversation(
        &mut self,
        agent_id: u64,
        conversation_id: ConversationId,
        partner_id: u64,
        social_act: String,
    ) -> Result<()> {
        self.clear_intent_navigation(agent_id)?;
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut conversation = entity_mut
            .get_mut::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?;
        conversation.active_conversation_id = Some(conversation_id);
        conversation.conversation_partner_id = Some(partner_id);
        conversation.last_social_act = Some(social_act);
        Ok(())
    }

    pub(super) fn release_agent_from_conversation(
        &mut self,
        agent_id: u64,
        social_act: String,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut conversation = entity_mut
            .get_mut::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?;
        conversation.active_conversation_id = None;
        conversation.conversation_partner_id = None;
        conversation.last_social_act = Some(social_act);
        conversation.social_cooldown_until = self.total_ticks + 2;
        Ok(())
    }

    pub(super) fn set_last_social_act(&mut self, agent_id: u64, social_act: String) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<ConversationComponent>()
            .ok_or_else(|| anyhow!("missing conversation component"))?
            .last_social_act = Some(social_act);
        Ok(())
    }
}

impl Simulation {}

impl Simulation {
    pub(super) fn apply_daily_aging(&mut self) -> Result<()> {
        let mut deceased = Vec::new();
        let mut grown_up_ids = Vec::new();
        {
            let mut query = self.world.query::<(
                Entity,
                &AgentCore,
                &mut LineageComponent,
                &LifeStatusComponent,
            )>();
            for (entity, core, mut lineage, status) in query.iter_mut(&mut self.world) {
                if status.0 == AgentLifeStatus::Vivo {
                    lineage.age += 6;
                    if lineage.age >= 18 && core.role_id == "crianca" {
                        grown_up_ids.push(core.id);
                    }
                    if lineage.age >= 70 {
                        use rand::Rng;
                        let mut rng = rand::rng();
                        let death_chance = if lineage.age >= 80 { 1.0 } else { 0.15 };
                        if rng.random::<f64>() < death_chance {
                            deceased.push((core.id, core.name.clone()));
                        }
                    }
                }
            }
        }
        for (agent_id, name) in deceased {
            self.mark_agent_dead(agent_id, "velhice")?;
        }
        for agent_id in grown_up_ids {
            self.grow_up_agent(agent_id)?;
        }
        Ok(())
    }

    pub(super) fn apply_daily_marriages(&mut self) -> Result<()> {
        let mut potentials = Vec::new();
        {
            let mut query = self.world.query::<(
                &AgentCore,
                &LineageComponent,
                &LifeStatusComponent,
                &RelationComponent,
            )>();
            for (core, lineage, status, relations) in query.iter(&self.world) {
                if status.0 == AgentLifeStatus::Vivo
                    && lineage.age >= 18
                    && lineage.spouse.is_none()
                {
                    potentials.push((core.id, lineage.gender.clone(), relations.0.clone()));
                }
            }
        }

        let mut marriages = Vec::new();
        let mut married_set = std::collections::HashSet::new();

        for i in 0..potentials.len() {
            let (id_a, ref gender_a, ref rel_a) = potentials[i];
            if married_set.contains(&id_a) {
                continue;
            }
            for j in (i + 1)..potentials.len() {
                let (id_b, ref gender_b, ref rel_b) = potentials[j];
                if married_set.contains(&id_b) {
                    continue;
                }
                if gender_a != gender_b {
                    let rel_ab = rel_a.get(&id_b);
                    let rel_ba = rel_b.get(&id_a);
                    if let (Some(ab), Some(ba)) = (rel_ab, rel_ba) {
                        if ab.attraction >= 20
                            && ab.friendship >= 15
                            && ba.attraction >= 20
                            && ba.friendship >= 15
                        {
                            marriages.push((id_a, id_b));
                            married_set.insert(id_a);
                            married_set.insert(id_b);
                            break;
                        }
                    }
                }
            }
        }

        for (id_a, id_b) in marriages {
            let name_a = self.agent_name(id_a)?;
            let name_b = self.agent_name(id_b)?;

            if let Ok(ent_a) = self.find_agent_entity(id_a) {
                let mut e_a = self.world.entity_mut(ent_a);
                if let Some(mut lin_a) = e_a.get_mut::<LineageComponent>() {
                    lin_a.spouse = Some(id_b);
                }
            }
            if let Ok(ent_b) = self.find_agent_entity(id_b) {
                let mut e_b = self.world.entity_mut(ent_b);
                if let Some(mut lin_b) = e_b.get_mut::<LineageComponent>() {
                    lin_b.spouse = Some(id_a);
                }
            }

            self.add_memory(
                id_a,
                MemoryKind::Success,
                format!("Casei-me com {}.", name_b),
                vec!["casamento".to_string(), "familia".to_string()],
                20,
                vec![id_b],
            )?;
            self.add_memory(
                id_b,
                MemoryKind::Success,
                format!("Casei-me com {}.", name_a),
                vec!["casamento".to_string(), "familia".to_string()],
                20,
                vec![id_a],
            )?;

            let ent_a = self.find_agent_entity(id_a)?;
            let home_a = self
                .world
                .entity(ent_a)
                .get::<AgentCore>()
                .unwrap()
                .home_building_id;
            let ent_b = self.find_agent_entity(id_b)?;
            let home_b = self
                .world
                .entity(ent_b)
                .get::<AgentCore>()
                .unwrap()
                .home_building_id;

            if let (Some(h_a), Some(h_b)) = (home_a, home_b) {
                if h_a != h_b {
                    let count_a = self
                        .households
                        .iter()
                        .find(|h| h.id == h_a)
                        .map(|h| h.member_ids.len())
                        .unwrap_or(3);
                    let count_b = self
                        .households
                        .iter()
                        .find(|h| h.id == h_b)
                        .map(|h| h.member_ids.len())
                        .unwrap_or(3);

                    if count_a < 3 {
                        if let Some(free_bed) = self.find_free_bed_in_building(h_a) {
                            if let Ok(ent_b) = self.find_agent_entity(id_b) {
                                let mut e_b = self.world.entity_mut(ent_b);
                                if let Some(mut core_b) = e_b.get_mut::<AgentCore>() {
                                    core_b.home_building_id = Some(h_a);
                                    core_b.home_bed = Some(free_bed);
                                }
                            }
                            if let Some(h_old) = self.households.iter_mut().find(|h| h.id == h_b) {
                                h_old.member_ids.retain(|&id| id != id_b);
                            }
                            if let Some(h_new) = self.households.iter_mut().find(|h| h.id == h_a) {
                                h_new.member_ids.push(id_b);
                            }
                            self.add_memory(
                                id_b,
                                MemoryKind::Impression,
                                format!("Mudei-me para a casa de meu cÃ´njuge {}.", name_a),
                                vec!["mudanca".to_string(), "familia".to_string()],
                                12,
                                vec![id_a],
                            )?;
                        }
                    } else if count_b < 3 {
                        if let Some(free_bed) = self.find_free_bed_in_building(h_b) {
                            if let Ok(ent_a) = self.find_agent_entity(id_a) {
                                let mut e_a = self.world.entity_mut(ent_a);
                                if let Some(mut core_a) = e_a.get_mut::<AgentCore>() {
                                    core_a.home_building_id = Some(h_b);
                                    core_a.home_bed = Some(free_bed);
                                }
                            }
                            if let Some(h_old) = self.households.iter_mut().find(|h| h.id == h_a) {
                                h_old.member_ids.retain(|&id| id != id_a);
                            }
                            if let Some(h_new) = self.households.iter_mut().find(|h| h.id == h_b) {
                                h_new.member_ids.push(id_a);
                            }
                            self.add_memory(
                                id_a,
                                MemoryKind::Impression,
                                format!("Mudei-me para a casa de meu cÃ´njuge {}.", name_b),
                                vec!["mudanca".to_string(), "familia".to_string()],
                                12,
                                vec![id_b],
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn apply_daily_births(&mut self) -> Result<()> {
        let mut potential_mothers = Vec::new();
        {
            let mut query = self
                .world
                .query::<(&AgentCore, &LineageComponent, &LifeStatusComponent)>();
            for (core, lineage, status) in query.iter(&self.world) {
                if status.0 == AgentLifeStatus::Vivo && lineage.gender == "Feminino" {
                    if let Some(spouse_id) = lineage.spouse {
                        potential_mothers.push((core.id, spouse_id, core.home_building_id));
                    }
                }
            }
        }

        println!(
            "DEBUG BIRTH: day={}, potential_mothers={:?}",
            self.day, potential_mothers
        );

        let mut parents_pairs = Vec::new();
        for (mother_id, spouse_id, home_building_id) in potential_mothers {
            if let Some(home_id) = home_building_id {
                if let Ok(spouse_ent) = self.find_agent_entity(spouse_id) {
                    let spouse_entry = self.world.entity(spouse_ent);
                    let spouse_status = spouse_entry.get::<LifeStatusComponent>().unwrap();
                    if spouse_status.0 == AgentLifeStatus::Vivo {
                        let spouse_core = spouse_entry.get::<AgentCore>().unwrap();
                        if spouse_core.home_building_id == Some(home_id) {
                            parents_pairs.push((mother_id, spouse_id, home_id));
                        }
                    }
                }
            }
        }

        println!(
            "DEBUG BIRTH: day={}, parents_pairs={:?}",
            self.day, parents_pairs
        );

        const CHILD_NAMES: &[&str] = &[
            "Arthur",
            "Alice",
            "Lucas",
            "Sofia",
            "Gabriel",
            "Beatriz",
            "Matheus",
            "Laura",
            "Bernardo",
            "Helena",
            "Pedro",
            "Valentina",
            "Miguel",
            "Manu",
            "Davi",
            "Isabela",
            "Tito",
            "Bela",
            "Otavio",
            "Clara",
            "Ruan",
            "Elena",
            "Vitor",
            "Livia",
            "Hugo",
            "Iris",
        ];

        for (mother_id, father_id, home_id) in parents_pairs {
            let member_count = self
                .households
                .iter()
                .find(|h| h.id == home_id)
                .map(|h| h.member_ids.len())
                .unwrap_or(3);
            if member_count >= 3 {
                continue;
            }

            use rand::Rng;
            let mut rng = rand::rng();
            if rng.random_bool(0.15) {
                let free_bed = self
                    .find_free_bed_in_building(home_id)
                    .unwrap_or_else(|| TileCoord { x: 0, y: 0 });

                let new_agent_id = self
                    .world
                    .query::<&AgentCore>()
                    .iter(&self.world)
                    .map(|c| c.id)
                    .max()
                    .unwrap_or(0)
                    + 1;

                let name = CHILD_NAMES[rng.random_range(0..CHILD_NAMES.len())].to_string();
                let child_gender = if rng.random_bool(0.5) {
                    "Masculino".to_string()
                } else {
                    "Feminino".to_string()
                };

                let mut child_traits = vec!["curioso".to_string()];
                let mut child_values = vec!["familia".to_string()];
                let mut child_fears = vec!["escassez".to_string()];

                if let Ok(mother_ent) = self.find_agent_entity(mother_id) {
                    let mother_entry = self.world.entity(mother_ent);
                    if let Some(prof) = mother_entry.get::<ProfileComponent>() {
                        if let Some(t) = prof.0.traits.first() {
                            child_traits.push(t.clone());
                        }
                        if let Some(v) = prof.0.values.first() {
                            child_values.push(v.clone());
                        }
                        if let Some(f) = prof.0.fears.first() {
                            child_fears.push(f.clone());
                        }
                    }
                }
                if let Ok(father_ent) = self.find_agent_entity(father_id) {
                    let father_entry = self.world.entity(father_ent);
                    if let Some(prof) = father_entry.get::<ProfileComponent>() {
                        if let Some(t) = prof.0.traits.first() {
                            child_traits.push(t.clone());
                        }
                        if let Some(v) = prof.0.values.first() {
                            child_values.push(v.clone());
                        }
                        if let Some(f) = prof.0.fears.first() {
                            child_fears.push(f.clone());
                        }
                    }
                }
                child_traits.dedup();
                child_values.dedup();
                child_fears.dedup();

                let mother_name = self.agent_name(mother_id)?;
                let father_name = self.agent_name(father_id)?;

                self.world.spawn((
                    (
                        AgentCore {
                            id: new_agent_id,
                            name: name.clone(),
                            role_id: "crianca".to_string(),
                            home_building_id: Some(home_id),
                            work_building_id: None,
                            home_bed: Some(free_bed),
                        },
                        ProfileComponent(AgentProfile {
                            traits: child_traits,
                            values: child_values,
                            fears: child_fears,
                            long_term_desires: vec!["crescer".to_string()],
                            moral_tolerances: vec!["mente por protecao".to_string()],
                            social_style: "submisso".to_string(),
                            trauma_traits: Vec::new(),
                        }),
                        StateComponent(AgentState {
                            mood: 80,
                            energy: 90,
                            health: 100,
                            hunger: 10,
                            stress: 0,
                            current_focus: "brincar".to_string(),
                            active_goals: vec!["crescer".to_string()],
                        }),
                        LifeStatusComponent(AgentLifeStatus::Vivo),
                        InjuryComponent::default(),
                        InstitutionalPerceptionComponent::default(),
                        PsychologicalStateComponent::default(),
                        RumorBeliefComponent::default(),
                        StoryBeliefComponent::default(),
                    ),
                    (
                        RelationComponent(HashMap::new()),
                        LineageComponent {
                            age: 0,
                            parents: vec![mother_id, father_id],
                            children: Vec::new(),
                            spouse: None,
                            gender: child_gender,
                            mourning_days_left: 0,
                        },
                        MemoryComponent(vec![AgentMemory {
                            id: new_agent_id * 100,
                            day: self.day,
                            tick: self.tick_of_day,
                            kind: MemoryKind::Fact,
                            summary: "Eu nasci.".to_string(),
                            details: format!(
                                "Nasci no lar de meus pais {} e {}.",
                                mother_name, father_name
                            ),
                            emotional_weight: 30,
                            about: vec![mother_id, father_id],
                            tags: vec!["nascimento".to_string(), "familia".to_string()],
                        }]),
                        InventoryComponent::default(),
                        ItemInventoryComponent::default(),
                        EquipmentComponent::default(),
                        CraftProficiencyComponent::default(),
                        PositionComponent(free_bed),
                    ),
                    (
                        DestinationComponent::default(),
                        DestinationLabelComponent::default(),
                        PathComponent::default(),
                        IntentComponent::default(),
                        TaskQueueComponent::default(),
                    ),
                    (
                        ThoughtComponent(format!("{} dorme tranquilamente em seu berco.", name)),
                        DecisionBudgetComponent::default(),
                        CognitionComponent {
                            next_reconsideration_tick: 0,
                            blocked_ticks: 0,
                            last_cognition_trigger: None,
                            last_social_opportunity_signature: None,
                            last_deliberation_hunger: 10,
                            last_deliberation_energy: 90,
                            last_deliberation_health: 100,
                            last_deliberation_stress: 0,
                        },
                        ConversationComponent::default(),
                        EconomicActivityComponent::default(),
                        TraumaTrackerComponent::default(),
                    ),
                ));

                if let Ok(mother_ent) = self.find_agent_entity(mother_id) {
                    let mut e_m = self.world.entity_mut(mother_ent);
                    if let Some(mut lin_m) = e_m.get_mut::<LineageComponent>() {
                        lin_m.children.push(new_agent_id);
                    }
                }
                if let Ok(father_ent) = self.find_agent_entity(father_id) {
                    let mut e_f = self.world.entity_mut(father_ent);
                    if let Some(mut lin_f) = e_f.get_mut::<LineageComponent>() {
                        lin_f.children.push(new_agent_id);
                    }
                }
                self.inherit_parental_stories(new_agent_id, mother_id, father_id, 35)?;

                self.add_memory(
                    mother_id,
                    MemoryKind::Success,
                    format!("Nasceu meu filho/filha {}.", name),
                    vec!["nascimento".to_string(), "familia".to_string()],
                    30,
                    vec![new_agent_id],
                )?;
                self.add_memory(
                    father_id,
                    MemoryKind::Success,
                    format!("Nasceu meu filho/filha {}.", name),
                    vec!["nascimento".to_string(), "familia".to_string()],
                    30,
                    vec![new_agent_id],
                )?;

                if let Some(h) = self.households.iter_mut().find(|h| h.id == home_id) {
                    h.member_ids.push(new_agent_id);
                }

                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: mother_id,
                    target: Some(new_agent_id),
                    kind: EventKind::SocialBond,
                    summary: format!(
                        "{name} nasceu na casa de {} e {}.",
                        mother_name, father_name
                    ),
                    impact_tags: vec!["nascimento".to_string(), "familia".to_string()],
                });
            }
        }
        Ok(())
    }

    pub(super) fn update_mourning_states(&mut self) -> Result<()> {
        let mut finished_mourning = Vec::new();
        let mut query = self
            .world
            .query::<(&AgentCore, &mut LineageComponent, &mut ProfileComponent)>();
        for (core, mut lineage, mut profile) in query.iter_mut(&mut self.world) {
            if lineage.mourning_days_left > 0 {
                lineage.mourning_days_left -= 1;
                if lineage.mourning_days_left == 0 {
                    profile.0.traits.retain(|t| t != "luto");
                    finished_mourning.push(core.id);
                }
            }
        }
        for agent_id in finished_mourning {
            self.add_memory(
                agent_id,
                MemoryKind::Reflection,
                "O luto passou. Preciso ser forte e continuar com minha vida.".to_string(),
                vec!["luto".to_string(), "aceitacao".to_string()],
                5,
                Vec::new(),
            )?;
        }
        Ok(())
    }

    pub(super) fn apply_child_behaviors(&mut self) -> Result<()> {
        let mut query = self.world.query::<(
            Entity,
            &AgentCore,
            &LifeStatusComponent,
            &mut StateComponent,
            &mut TaskQueueComponent,
        )>();
        for (entity, core, status, mut state, mut queue) in query.iter_mut(&mut self.world) {
            if status.0 == AgentLifeStatus::Vivo && core.role_id == "crianca" {
                if queue.0.is_empty() {
                    let mut tasks = Vec::new();
                    if state.0.hunger >= 65 {
                        tasks.push(SimplifiedTask {
                            kind: IntentKind::Comer,
                            target_semantic: Some("casa".to_string()),
                            target_agent: None,
                            social_move: None,
                        });
                    } else if state.0.energy <= 25 {
                        tasks.push(SimplifiedTask {
                            kind: IntentKind::Descansar,
                            target_semantic: None,
                            target_agent: None,
                            social_move: None,
                        });
                    } else {
                        use rand::Rng;
                        let mut rng = rand::rng();
                        let places = vec!["praca", "casa"];
                        let idx = rng.random_range(0..places.len());
                        tasks.push(SimplifiedTask {
                            kind: IntentKind::Andar,
                            target_semantic: Some(places[idx].to_string()),
                            target_agent: None,
                            social_move: None,
                        });
                        tasks.push(SimplifiedTask {
                            kind: IntentKind::Refletir,
                            target_semantic: None,
                            target_agent: None,
                            social_move: None,
                        });
                    }
                    queue.0.extend(tasks);
                }
            }
        }
        Ok(())
    }

    pub(super) fn grow_up_agent(&mut self, agent_id: u64) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut new_role = "campones".to_string();
        let mut new_work = None;

        let (parents, core_id) = {
            let entry = self.world.entity(entity);
            let lineage = entry.get::<LineageComponent>().unwrap();
            let core = entry.get::<AgentCore>().unwrap();
            (lineage.parents.clone(), core.id)
        };

        let mut parent_candidates = Vec::new();
        for parent_id in parents.clone() {
            if let Ok(parent_ent) = self.find_agent_entity(parent_id) {
                let parent_entry = self.world.entity(parent_ent);
                let parent_status = parent_entry.get::<LifeStatusComponent>().unwrap();
                if parent_status.0 == AgentLifeStatus::Morto {
                    let parent_core = parent_entry.get::<AgentCore>().unwrap();
                    let parent_role = parent_core.role_id.clone();
                    let parent_work = parent_core.work_building_id;
                    if parent_role != "campones" && parent_role != "crianca" {
                        parent_candidates.push((parent_role, parent_work));
                    }
                }
            }
        }

        let village_id = (core_id - 1) / 7;
        for (parent_role, parent_work) in parent_candidates {
            let mut job_taken = false;
            let mut q_others = self.world.query::<(&AgentCore, &LifeStatusComponent)>();
            for (other_core, other_status) in q_others.iter(&self.world) {
                let other_village = (other_core.id - 1) / 7;
                if other_village == village_id
                    && other_status.0 == AgentLifeStatus::Vivo
                    && other_core.role_id == parent_role
                {
                    job_taken = true;
                    break;
                }
            }
            if !job_taken {
                new_role = parent_role;
                new_work = parent_work;
                break;
            }
        }

        let mut entity_mut = self.world.entity_mut(entity);
        let name = {
            let mut core = entity_mut.get_mut::<AgentCore>().unwrap();
            core.role_id = new_role.clone();
            core.work_building_id = new_work;
            core.name.clone()
        };

        if let Some(mut profile) = entity_mut.get_mut::<ProfileComponent>() {
            profile.0.traits.retain(|t| t != "curioso");
            profile.0.traits.push("trabalhador".to_string());
            profile.0.long_term_desires = vec!["acumular riqueza".to_string()];
        }

        if let Some(mut state) = entity_mut.get_mut::<StateComponent>() {
            state.0.current_focus = "trabalhar".to_string();
            state.0.active_goals = vec!["sobreviver".to_string()];
        }

        self.add_memory(
            agent_id,
            MemoryKind::Success,
            format!("Atingi a maioridade e assumi o papel de {}.", new_role),
            vec!["crescimento".to_string(), "maioridade".to_string()],
            15,
            Vec::new(),
        )?;

        if parents.len() >= 2 {
            self.inherit_parental_stories(agent_id, parents[0], parents[1], 20)?;
        }

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::FactionShift,
            summary: format!("{name} atingiu a maioridade e tornou-se {}.", new_role),
            impact_tags: vec!["crescimento".to_string(), "papel".to_string()],
        });

        Ok(())
    }

    pub(super) fn find_free_bed_in_building(
        &mut self,
        building_id: BuildingId,
    ) -> Option<TileCoord> {
        let mut beds = Vec::new();
        for fixture in &self.spatial.fixtures {
            if fixture.building_id == Some(building_id) && fixture.kind == FixtureKind::Bed {
                beds.push(fixture.coord);
            }
        }
        let mut occupied = std::collections::HashSet::new();
        let mut query = self.world.query::<(&AgentCore, &LifeStatusComponent)>();
        for (core, status) in query.iter(&self.world) {
            if status.0 == AgentLifeStatus::Vivo {
                if let Some(bed) = core.home_bed {
                    if core.home_building_id == Some(building_id) {
                        occupied.insert(bed);
                    }
                }
            }
        }
        for bed in beds {
            if !occupied.contains(&bed) {
                return Some(bed);
            }
        }
        None
    }
}
