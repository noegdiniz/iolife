use super::*;
use std::collections::HashSet;
// Political pressure, faction, issue and local norm systems.

impl Simulation {
    pub(super) fn policy_act_is_active(&self, act: &PolicyAct) -> bool {
        act.status == PolicyActStatus::Active
            && act
                .expires_day
                .is_none_or(|expires_day| expires_day >= self.day)
    }

    pub(super) fn active_policy_effects(&self) -> Vec<&PolicyEffect> {
        self.policy_acts
            .iter()
            .filter(|act| self.policy_act_is_active(act))
            .flat_map(|act| act.effects.iter())
            .collect()
    }

    pub(super) fn has_active_policy_effect(
        &self,
        mut predicate: impl FnMut(&PolicyEffect) -> bool,
    ) -> bool {
        self.policy_acts
            .iter()
            .filter(|act| self.policy_act_is_active(act))
            .flat_map(|act| act.effects.iter())
            .any(|effect| predicate(effect))
    }

    pub(super) fn active_policy_act_by_agenda(&self, agenda_tag: &str) -> Option<&PolicyAct> {
        self.policy_acts
            .iter()
            .find(|act| act.agenda_tag == agenda_tag && self.policy_act_is_active(act))
    }

    pub(super) fn active_edict_tags(&self) -> Vec<String> {
        self.policy_acts
            .iter()
            .filter(|act| {
                self.policy_act_is_active(act)
                    && matches!(act.authority, PolicyAuthority::LocalLeader)
            })
            .map(|act| act.agenda_tag.clone())
            .collect()
    }

    pub(super) fn active_tax_multiplier_percent(&self) -> i32 {
        self.active_policy_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                PolicyEffect::TaxModifier { multiplier_percent } => Some(*multiplier_percent),
                _ => None,
            })
            .max()
            .unwrap_or(100)
    }

    pub(super) fn active_rationing_energy_gain_percent(&self) -> i32 {
        self.active_policy_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                PolicyEffect::RationingRule {
                    energy_gain_percent,
                    ..
                } => Some(*energy_gain_percent),
                _ => None,
            })
            .min()
            .unwrap_or(100)
    }

    pub(super) fn movement_restricted_establishment_types(&self) -> Vec<String> {
        self.active_policy_effects()
            .into_iter()
            .filter_map(|effect| match effect {
                PolicyEffect::MovementRestriction {
                    establishment_type_id,
                } => Some(establishment_type_id.clone()),
                _ => None,
            })
            .collect()
    }

    pub(super) fn policy_effects_for_edict_tag(edital_tag: &str) -> Vec<PolicyEffect> {
        match edital_tag {
            "trabalho_forcado_campos" => vec![PolicyEffect::LaborDraft {
                output_resource_id: "graos".to_string(),
                production_bonus_percent: 100,
            }],
            "racionamento_estrito" => vec![PolicyEffect::RationingRule {
                policy: RationingPolicy::Balanced,
                energy_gain_percent: 70,
            }],
            "imposto_guerra" => vec![
                PolicyEffect::TaxModifier {
                    multiplier_percent: 200,
                },
                PolicyEffect::Mobilization {
                    readiness_delta: 20,
                },
            ],
            "proibicao_tavernas" => vec![PolicyEffect::MovementRestriction {
                establishment_type_id: "taverna".to_string(),
            }],
            "confisco_metais" => vec![PolicyEffect::ResourceConfiscation {
                resource_id: "metal_bruto".to_string(),
                excluded_establishment_type_ids: vec![
                    "solar".to_string(),
                    "posto_guarda".to_string(),
                ],
                destination_establishment_type_id: "solar".to_string(),
            }],
            _ => Vec::new(),
        }
    }

    pub(super) fn edict_summary(edital_tag: &str) -> String {
        match edital_tag {
            "trabalho_forcado_campos" => "Edital Real: Trabalho forcado nos campos".to_string(),
            "racionamento_estrito" => "Edital Real: Racionamento estrito de graos".to_string(),
            "imposto_guerra" => "Edital Real: Imposto de guerra dobrado".to_string(),
            "proibicao_tavernas" => "Edital Real: Proibicao de tavernas no vilarejo".to_string(),
            "confisco_metais" => "Edital Real: Confisco geral de metais".to_string(),
            "reduzir_imposto" => "Decreto local: reduzir imposto diario".to_string(),
            "aumentar_imposto" => "Decreto local: aumentar imposto diario".to_string(),
            "justica_branda" => "Decreto local: tornar a justica branda".to_string(),
            "justica_normal" => "Decreto local: tornar a justica normal".to_string(),
            "justica_severa" => "Decreto local: tornar a justica severa".to_string(),
            "racionamento_lares" => "Decreto local: priorizar lares no racionamento".to_string(),
            "racionamento_produtores" => {
                "Decreto local: priorizar produtores no racionamento".to_string()
            }
            "racionamento_civico" => {
                "Decreto local: priorizar setor civico no racionamento".to_string()
            }
            "racionamento_equilibrado" => {
                "Decreto local: racionamento alimentar equilibrado".to_string()
            }
            _ => format!("Edital Real: {}", edital_tag),
        }
    }

    pub(super) fn policy_domain_for_decree_tag(edital_tag: &str) -> PolicyDomain {
        match edital_tag {
            "racionamento_estrito"
            | "racionamento_lares"
            | "racionamento_produtores"
            | "racionamento_civico"
            | "racionamento_equilibrado" => PolicyDomain::Rationing,
            "imposto_guerra" | "reduzir_imposto" | "aumentar_imposto" => PolicyDomain::Tax,
            _ => PolicyDomain::Justice,
        }
    }

    pub(super) fn apply_decree_norm_change(&mut self, edital_tag: &str) -> Result<bool> {
        let before = format!(
            "imposto={} justica={} racionamento={}",
            self.village_economy.daily_household_tax,
            self.local_norms.justice_severity.as_str(),
            self.local_norms.rationing_policy.as_str()
        );
        let changed = match edital_tag {
            "reduzir_imposto" => {
                self.village_economy.daily_household_tax =
                    (self.village_economy.daily_household_tax - 1).max(1);
                true
            }
            "aumentar_imposto" => {
                self.village_economy.daily_household_tax =
                    (self.village_economy.daily_household_tax + 1).min(5);
                true
            }
            "justica_branda" => {
                self.local_norms.justice_severity = JusticeSeverity::Lenient;
                true
            }
            "justica_normal" => {
                self.local_norms.justice_severity = JusticeSeverity::Normal;
                true
            }
            "justica_severa" => {
                self.local_norms.justice_severity = JusticeSeverity::Severe;
                true
            }
            "racionamento_lares" => {
                self.local_norms.rationing_policy = RationingPolicy::HouseholdFirst;
                true
            }
            "racionamento_produtores" => {
                self.local_norms.rationing_policy = RationingPolicy::ProducersFirst;
                true
            }
            "racionamento_civico" => {
                self.local_norms.rationing_policy = RationingPolicy::CivicFirst;
                true
            }
            "racionamento_equilibrado" => {
                self.local_norms.rationing_policy = RationingPolicy::Balanced;
                true
            }
            _ => false,
        };
        if changed {
            let after = format!(
                "imposto={} justica={} racionamento={}",
                self.village_economy.daily_household_tax,
                self.local_norms.justice_severity.as_str(),
                self.local_norms.rationing_policy.as_str()
            );
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: 0,
                target: None,
                kind: EventKind::NormChanged,
                summary: format!("Norma local alterada por decreto: {before} -> {after}."),
                impact_tags: vec!["politica".to_string(), "decreto".to_string()],
            });
        }
        Ok(changed)
    }

    pub(super) fn apply_political_support_intent(
        &mut self,
        actor_id: u64,
        support: bool,
    ) -> Result<()> {
        let Some(issue_id) = self.preferred_political_issue_for_actor(actor_id) else {
            return Ok(());
        };
        if !self.record_political_position(actor_id, issue_id, support)? {
            return Ok(());
        }
        let actor_name = self.agent_name(actor_id)?;
        let issue_summary = self
            .political_issues
            .iter()
            .find(|issue| issue.id == issue_id)
            .map(|issue| issue.summary.clone())
            .unwrap_or_else(|| format!("pauta {issue_id}"));
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: None,
            kind: EventKind::PoliticalSupport,
            summary: format!(
                "{actor_name} {} a pauta: {issue_summary}.",
                if support { "apoia" } else { "se opoe a" }
            ),
            impact_tags: vec!["politica".to_string(), "apoio".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_political_pressure_intent(
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
        let Some(issue_id) = self.preferred_political_issue_for_actor(actor_id) else {
            return Ok(());
        };
        let _actor_changed = self.record_political_position(actor_id, issue_id, true)?;
        let influence = (self.political_influence(actor_id) / 3).max(1);
        if let Some(issue) = self
            .political_issues
            .iter_mut()
            .find(|issue| issue.id == issue_id)
        {
            issue.support_score += influence;
        }
        self.apply_relation_delta(
            target_id,
            actor_id,
            &RelationDelta {
                trust: -3,
                friendship: -2,
                resentment: 5,
                attraction: 0,
                moral_debt: -2,
                reputation: -1,
            },
        )?;
        let actor_name = self.agent_name(actor_id)?;
        let target_name = self.agent_name(target_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: Some(target_id),
            kind: EventKind::InstitutionalDispute,
            summary: format!("{actor_name} pressiona {target_name} em disputa institucional."),
            impact_tags: vec!["politica".to_string(), "pressao".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_political_request_support_intent(
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
        let Some(issue_id) = self.preferred_political_issue_for_actor(actor_id) else {
            return Ok(());
        };
        let actor_changed = self.record_political_position(actor_id, issue_id, true)?;
        let relation = self.relation_between(target_id, actor_id);
        let persuaded = relation.trust + relation.friendship - relation.resentment >= -10;
        let target_changed = self.record_political_position(target_id, issue_id, persuaded)?;
        if !actor_changed && !target_changed {
            return Ok(());
        }
        self.apply_relation_delta(
            actor_id,
            target_id,
            &RelationDelta {
                trust: if persuaded { 2 } else { -2 },
                friendship: if persuaded { 2 } else { -1 },
                resentment: if persuaded { -1 } else { 3 },
                attraction: 0,
                moral_debt: if persuaded { 1 } else { 0 },
                reputation: 0,
            },
        )?;
        let actor_name = self.agent_name(actor_id)?;
        let target_name = self.agent_name(target_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: Some(target_id),
            kind: EventKind::PoliticalSupport,
            summary: if persuaded {
                format!("{actor_name} convence {target_name} a apoiar uma pauta local.")
            } else {
                format!("{actor_name} pede apoio a {target_name}, mas encontra resistencia.")
            },
            impact_tags: vec!["politica".to_string(), "apoio".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_political_mediate_intent(
        &mut self,
        actor_id: u64,
        target_id: Option<u64>,
    ) -> Result<()> {
        if let Some(target_id) = target_id
            && !self.agents_adjacent(actor_id, target_id)?
        {
            return Ok(());
        }
        if self.political_influence(actor_id) < 18 {
            return Ok(());
        }
        let Some(issue) = self
            .political_issues
            .iter_mut()
            .filter(|issue| issue.status == PoliticalIssueStatus::Open)
            .max_by_key(|issue| (issue.support_score - issue.opposition_score).abs())
        else {
            return Ok(());
        };
        issue.support_score = (issue.support_score - 4).max(0);
        issue.opposition_score = (issue.opposition_score - 4).max(0);
        let actor_name = self.agent_name(actor_id)?;
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: actor_id,
            target: target_id,
            kind: EventKind::InstitutionalDispute,
            summary: format!("{actor_name} medeia uma disputa politica e reduz a polarizacao."),
            impact_tags: vec!["politica".to_string(), "mediacao".to_string()],
        });
        Ok(())
    }

    pub(super) fn institutional_perception(
        &mut self,
        agent_id: u64,
    ) -> Option<InstitutionalPerception> {
        let entity = self.find_agent_entity(agent_id).ok()?;
        self.world
            .entity(entity)
            .get::<InstitutionalPerceptionComponent>()
            .map(|component| component.0.clone())
    }

    pub(super) fn adjust_institutional_perception(
        &mut self,
        agent_id: u64,
        delta: InstitutionalPerception,
        note: impl Into<String>,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        let mut component = entity_mut
            .get_mut::<InstitutionalPerceptionComponent>()
            .ok_or_else(|| anyhow!("missing institutional perception component"))?;
        component.0.leader_legitimacy += delta.leader_legitimacy;
        component.0.justice_legitimacy += delta.justice_legitimacy;
        component.0.tax_legitimacy += delta.tax_legitimacy;
        component.0.rationing_legitimacy += delta.rationing_legitimacy;
        component.0.guard_trust += delta.guard_trust;
        component.0.war_support += delta.war_support;
        component.0.fear_of_authority += delta.fear_of_authority;
        component.0.perceived_corruption += delta.perceived_corruption;
        component.0.perceived_fairness += delta.perceived_fairness;
        component.0.last_updated_day = self.day;
        let note = note.into();
        if !note.is_empty() {
            component.0.notes.push(note);
        }
        component.0.clamp_all();
        Ok(())
    }

    pub(super) fn build_institutional_context(
        &mut self,
        agent_id: u64,
    ) -> InstitutionalContextInput {
        let perception = self.institutional_perception(agent_id).unwrap_or_default();
        let mut likely_reactions = Vec::new();
        if perception.guard_trust <= -25 {
            likely_reactions.push("desconfia dos guardas e evita denunciar crimes".to_string());
        } else if perception.guard_trust >= 30 {
            likely_reactions.push("tende a procurar guardas quando ameacado".to_string());
        }
        if perception.tax_legitimacy <= -25 {
            likely_reactions.push("acha imposto injusto e pode apoiar boicote fiscal".to_string());
        }
        if perception.justice_legitimacy <= -25 {
            likely_reactions.push("ve a justica local como corrupta ou arbitraria".to_string());
        }
        if perception.rationing_legitimacy <= -25 {
            likely_reactions
                .push("acha o racionamento injusto e pode apoiar motim por comida".to_string());
        }
        if perception.war_support <= -25 {
            likely_reactions.push("rejeita o custo da guerra e pode culpar o lider".to_string());
        } else if perception.war_support >= 30 {
            likely_reactions.push("apoia esforco de guerra se parecer defensivo".to_string());
        }
        if perception.fear_of_authority >= 45 && perception.leader_legitimacy <= -20 {
            likely_reactions.push("obedece por medo, mas pode conspirar em segredo".to_string());
        }
        if likely_reactions.is_empty() {
            likely_reactions.push("sem postura institucional extrema no momento".to_string());
        }
        let summary = format!(
            "lider={} justica={} imposto={} racionamento={} guardas={} guerra={} medo={} corrupcao={} equidade={}",
            perception.leader_legitimacy,
            perception.justice_legitimacy,
            perception.tax_legitimacy,
            perception.rationing_legitimacy,
            perception.guard_trust,
            perception.war_support,
            perception.fear_of_authority,
            perception.perceived_corruption,
            perception.perceived_fairness
        );
        InstitutionalContextInput {
            leader_legitimacy: perception.leader_legitimacy,
            justice_legitimacy: perception.justice_legitimacy,
            tax_legitimacy: perception.tax_legitimacy,
            rationing_legitimacy: perception.rationing_legitimacy,
            guard_trust: perception.guard_trust,
            war_support: perception.war_support,
            fear_of_authority: perception.fear_of_authority,
            perceived_corruption: perception.perceived_corruption,
            perceived_fairness: perception.perceived_fairness,
            summary,
            likely_reactions,
        }
    }

    pub(super) fn active_feudal_title_for_holder(&self, agent_id: u64) -> Option<&FeudalTitle> {
        self.feudal_titles
            .iter()
            .filter(|title| title.active && title.holder_agent_id == Some(agent_id))
            .max_by_key(|title| title.precedence)
    }

    pub(super) fn direct_lord_for_agent(&self, agent_id: u64) -> Option<u64> {
        self.feudal_contracts
            .iter()
            .filter(|contract| {
                contract.vassal_agent_id == agent_id
                    && contract.status == FeudalContractStatus::Active
            })
            .max_by_key(|contract| contract.loyalty + contract.coercion)
            .map(|contract| contract.suzerain_agent_id)
    }

    pub(super) fn feudal_holdings_for_agent(&self, agent_id: u64) -> Vec<&EstateHolding> {
        self.estate_holdings
            .iter()
            .filter(|holding| holding.holder_agent_id == Some(agent_id))
            .collect()
    }

    pub(super) fn territory_for_agent(&self, agent_id: u64) -> Option<TerritoryId> {
        let core = self.agent_core_snapshot(agent_id)?;
        let position_building_id = self
            .find_agent_entity(agent_id)
            .ok()
            .and_then(|entity| {
                self.world
                    .get::<PositionComponent>(entity)
                    .map(|position| position.0)
            })
            .and_then(|coord| self.tile_at(coord).and_then(|tile| tile.building_id));
        let building_id = core
            .home_building_id
            .or(core.work_building_id)
            .or(position_building_id);
        building_id.and_then(|building_id| {
            self.territories
                .iter()
                .find(|territory| territory.building_ids.contains(&building_id))
                .map(|territory| territory.id)
        })
    }

    fn agent_core_snapshot(&self, agent_id: u64) -> Option<AgentCore> {
        let entity = self.find_agent_entity(agent_id).ok()?;
        self.world.get::<AgentCore>(entity).cloned()
    }

    fn lineage_snapshot(&self, agent_id: u64) -> Option<LineageComponent> {
        let entity = self.find_agent_entity(agent_id).ok()?;
        self.world.get::<LineageComponent>(entity).cloned()
    }

    pub(super) fn subordinates_for_agent(&self, agent_id: u64) -> Vec<u64> {
        let mut subordinates = self
            .feudal_contracts
            .iter()
            .filter(|contract| {
                contract.suzerain_agent_id == agent_id
                    && contract.status == FeudalContractStatus::Active
            })
            .map(|contract| contract.vassal_agent_id)
            .collect::<Vec<_>>();
        for office in self
            .authority_offices
            .iter()
            .filter(|office| office.active && office.granter_agent_id == Some(agent_id))
        {
            if let Some(holder) = office.holder_agent_id {
                subordinates.push(holder);
            }
        }
        subordinates.sort_unstable();
        subordinates.dedup();
        subordinates
    }

    pub(super) fn feudal_power_for_agent(&self, agent_id: u64) -> i32 {
        let title_power = self
            .active_feudal_title_for_holder(agent_id)
            .map(|title| title.precedence + title.legitimacy / 2)
            .unwrap_or(0);
        let holding_power = self
            .feudal_holdings_for_agent(agent_id)
            .into_iter()
            .map(|holding| holding.annualized_value / 12)
            .sum::<i32>();
        let contract_power = self
            .feudal_contracts
            .iter()
            .filter(|contract| {
                contract.suzerain_agent_id == agent_id
                    && contract.status == FeudalContractStatus::Active
            })
            .map(|contract| (contract.loyalty + contract.coercion) / 4)
            .sum::<i32>();
        let office_power = self
            .authority_offices
            .iter()
            .filter(|office| office.active && office.holder_agent_id == Some(agent_id))
            .map(|office| office.authority_score)
            .sum::<i32>();
        title_power + holding_power + contract_power + office_power
    }

    pub(super) fn feudal_war_support_for_polity(&self, polity_id: PolityId) -> i32 {
        let title_holders = self
            .feudal_titles
            .iter()
            .filter(|title| title.active && title.polity_id == Some(polity_id))
            .filter_map(|title| title.holder_agent_id)
            .collect::<HashSet<_>>();
        let holdings_power = self
            .estate_holdings
            .iter()
            .filter(|holding| holding.holder_polity_id == Some(polity_id))
            .map(|holding| holding.annualized_value / 18 + holding.military_obligation * 3)
            .sum::<i32>();
        let contract_support = self
            .feudal_contracts
            .iter()
            .filter(|contract| title_holders.contains(&contract.suzerain_agent_id))
            .map(|contract| match contract.status {
                FeudalContractStatus::Active => {
                    contract.levy_duty * 4
                        + contract.maintenance_duty
                        + contract.loyalty / 5
                        + contract.coercion / 8
                }
                FeudalContractStatus::Breached => -(contract.levy_duty * 3 + 6),
                FeudalContractStatus::Revoked => -(contract.levy_duty * 2 + 4),
            })
            .sum::<i32>();
        let office_support = self
            .authority_offices
            .iter()
            .filter(|office| office.active && office.holder_agent_id.is_some())
            .filter(|office| {
                office
                    .title_id
                    .and_then(|title_id| {
                        self.feudal_titles
                            .iter()
                            .find(|title| title.id == title_id)
                            .and_then(|title| title.polity_id)
                    })
                    == Some(polity_id)
            })
            .map(|office| office.authority_score / 3)
            .sum::<i32>();
        let succession_penalty = self
            .succession_crises
            .iter()
            .filter(|crisis| crisis.status == SuccessionCrisisStatus::Open)
            .filter(|crisis| {
                self.feudal_titles
                    .iter()
                    .find(|title| title.id == crisis.title_id)
                    .and_then(|title| title.polity_id)
                    == Some(polity_id)
            })
            .map(|crisis| crisis.conflict_score / 4 + crisis.legitimacy_gap / 5)
            .sum::<i32>();
        let arrears_penalty = self
            .households
            .iter()
            .filter(|household| {
                household
                    .direct_lord_agent_id
                    .is_some_and(|lord_id| title_holders.contains(&lord_id))
            })
            .map(|household| household.feudal_arrears / 2)
            .sum::<i32>();
        (holdings_power + contract_support + office_support - succession_penalty - arrears_penalty)
            .clamp(-35, 45)
    }

    pub(super) fn build_feudal_context(&self, agent_id: u64) -> FeudalContextInput {
        let title = self.active_feudal_title_for_holder(agent_id).cloned();
        let direct_lord_id = self.direct_lord_for_agent(agent_id);
        let subordinate_ids = self.subordinates_for_agent(agent_id);
        let holdings = self
            .feudal_holdings_for_agent(agent_id)
            .into_iter()
            .map(|holding| {
                let territory = self
                    .territories
                    .iter()
                    .find(|territory| territory.id == holding.territory_id)
                    .map(|territory| territory.name.clone())
                    .unwrap_or_else(|| format!("territorio {}", holding.territory_id));
                format!(
                    "{} em {} | valor={} | tributo={}%",
                    holding.name,
                    territory,
                    holding.annualized_value,
                    holding.tribute_share_percent
                )
            })
            .collect::<Vec<_>>();
        let obligations = self
            .feudal_contracts
            .iter()
            .filter(|contract| {
                contract.vassal_agent_id == agent_id
                    && contract.status == FeudalContractStatus::Active
            })
            .map(|contract| {
                format!(
                    "tributo={} levy={} manutencao={} lealdade={} coercao={}",
                    contract.tribute_due_per_day,
                    contract.levy_duty,
                    contract.maintenance_duty,
                    contract.loyalty,
                    contract.coercion
                )
            })
            .collect::<Vec<_>>();
        let contract_pressures = self
            .feudal_contracts
            .iter()
            .filter(|contract| {
                (contract.vassal_agent_id == agent_id || contract.suzerain_agent_id == agent_id)
                    && contract.status != FeudalContractStatus::Revoked
            })
            .map(|contract| {
                let relation = if contract.vassal_agent_id == agent_id {
                    "deve ao suserano"
                } else {
                    "cobra do vassalo"
                };
                format!(
                    "{} | legitimidade={} status={:?}",
                    relation, contract.perceived_legitimacy, contract.status
                )
            })
            .take(3)
            .collect::<Vec<_>>();
        let succession_status = self
            .succession_crises
            .iter()
            .filter(|crisis| {
                crisis.status == SuccessionCrisisStatus::Open
                    && (crisis.claimant_ids.contains(&agent_id)
                        || self
                            .active_feudal_title_for_holder(agent_id)
                            .is_some_and(|title| title.id == crisis.title_id))
            })
            .map(|crisis| {
                format!(
                    "crise sucessoria #{} conflito={} legitimidade_gap={} | {}",
                    crisis.id, crisis.conflict_score, crisis.legitimacy_gap, crisis.summary
                )
            })
            .collect::<Vec<_>>();
        let authority_conflicts = self
            .power_centers
            .iter()
            .filter(|center| center.agent_id == Some(agent_id))
            .filter(|center| (center.formal_authority - center.material_power).abs() >= 20)
            .map(|center| center.summary.clone())
            .take(3)
            .collect::<Vec<_>>();
        let sanction_risk = if let Some(household) = self
            .household_id_for_agent_immutable(agent_id)
            .and_then(|id| self.household_by_id(id))
        {
            if household.feudal_arrears > 0 {
                format!(
                    "risco de sancao feudal por atrasos={} e tributo devido={}",
                    household.feudal_arrears, household.feudal_tribute_due
                )
            } else {
                "risco feudal imediato baixo".to_string()
            }
        } else {
            "sem carga feudal direta mapeada".to_string()
        };
        let power_summary = if let Some(title) = &title {
            format!(
                "{} como {} tem poder feudal {} e {} subordinado(s).",
                self.agent_name(agent_id)
                    .unwrap_or_else(|_| format!("Agente {}", agent_id)),
                title.rank.as_str(),
                self.feudal_power_for_agent(agent_id),
                subordinate_ids.len()
            )
        } else if direct_lord_id.is_some() {
            format!(
                "depende de autoridade superior e possui poder feudal limitado ({})",
                self.feudal_power_for_agent(agent_id)
            )
        } else {
            format!(
                "sem titulo formal relevante; poder feudal atual {}",
                self.feudal_power_for_agent(agent_id)
            )
        };
        FeudalContextInput {
            title: title.as_ref().map(|title| title.name.clone()),
            direct_lord: direct_lord_id.and_then(|id| self.agent_name(id).ok()),
            subordinate_summaries: subordinate_ids
                .into_iter()
                .filter_map(|id| self.agent_name(id).ok())
                .map(|name| format!("subordinado: {name}"))
                .collect(),
            holdings,
            obligations,
            contract_pressures,
            succession_status,
            authority_conflicts,
            sanction_risk,
            power_summary,
        }
    }

    pub(super) fn build_political_context(&self, context: &AgentContext) -> PoliticalContextInput {
        let mut local_norms = vec![
            format!(
                "imposto diario por lar: {} moeda(s)",
                self.village_economy.daily_household_tax
            ),
            format!("justica: {}", self.local_norms.justice_severity.as_str()),
            format!(
                "racionamento alimentar: {}",
                self.local_norms.rationing_policy.as_str()
            ),
        ];

        // Adicionar as leis e editais reais conhecidos
        let role_id = &context.role_id;
        let memories = &context.memories;
        for act in self
            .policy_acts
            .iter()
            .filter(|act| self.policy_act_is_active(act))
        {
            if matches!(act.authority, PolicyAuthority::LocalLeader) {
                let agente_sabe = role_id == "lider_local"
                    || role_id == "guarda"
                    || memories.iter().any(|m| m.tags.contains(&act.agenda_tag));
                if agente_sabe {
                    local_norms.push(format!(
                        "Edital Real Ativo: {} ({})",
                        act.summary, act.agenda_tag
                    ));
                }
            }
        }
        for war in self
            .wars
            .iter()
            .filter(|war| war.status == WarStatus::Active)
            .take(3)
        {
            local_norms.push(format!(
                "Guerra ativa #{} {:?}: atacante={} defensor={} placar {}-{}",
                war.id,
                war.stage,
                war.attacker_polity_id,
                war.defender_polity_id,
                war.attacker_score,
                war.defender_score
            ));
        }
        for insurrection in self
            .insurrections
            .iter()
            .filter(|insurrection| insurrection.status == InsurrectionStatus::Active)
            .take(3)
        {
            local_norms.push(format!(
                "Insurreicao #{} {:?}: apoio={} repressao={} faccoes={:?}",
                insurrection.id,
                insurrection.stage,
                insurrection.popular_support,
                insurrection.repression,
                insurrection.faction_ids
            ));
        }
        let grievances = self.political_grievances_for_agent(context.id);
        let relevant_factions = self
            .political_factions
            .iter()
            .filter(|faction| {
                faction.member_ids.contains(&context.id)
                    || grievances
                        .iter()
                        .any(|grievance| grievance.contains(&faction.agenda_tag))
            })
            .take(4)
            .map(|faction| {
                format!(
                    "#{} {} influencia={} membros={}",
                    faction.id,
                    faction.name,
                    faction.influence,
                    faction.member_ids.len()
                )
            })
            .collect::<Vec<_>>();
        let open_issues = self
            .political_issues
            .iter()
            .filter(|issue| issue.status == PoliticalIssueStatus::Open)
            .take(5)
            .map(|issue| {
                format!(
                    "#{} {} -> {} | apoio={} oposicao={}",
                    issue.id,
                    issue.domain.as_str(),
                    issue.proposed_value,
                    issue.support_score,
                    issue.opposition_score
                )
            })
            .collect::<Vec<_>>();
        let opposition_risks = self
            .political_issues
            .iter()
            .filter(|issue| {
                issue.status == PoliticalIssueStatus::Open
                    && (issue.supporter_ids.contains(&context.id)
                        || issue.opposer_ids.contains(&context.id))
            })
            .take(3)
            .map(|issue| format!("pauta #{} pode gerar oposicao social", issue.id))
            .collect::<Vec<_>>();
        PoliticalContextInput {
            local_norms,
            relevant_factions,
            open_issues,
            likely_position: self.political_position_for_agent(context.id),
            household_grievances: grievances,
            opposition_risks,
        }
    }

    pub(super) fn political_position_for_agent(&self, agent_id: u64) -> String {
        let mut positions = Vec::new();
        for issue in self
            .political_issues
            .iter()
            .filter(|issue| issue.status == PoliticalIssueStatus::Open)
        {
            if issue.supporter_ids.contains(&agent_id) {
                positions.push(format!("apoia #{} {}", issue.id, issue.agenda_tag));
            } else if issue.opposer_ids.contains(&agent_id) {
                positions.push(format!("opoe #{} {}", issue.id, issue.agenda_tag));
            }
        }
        if positions.is_empty() {
            self.political_pressures
                .iter()
                .find(|pressure| pressure.actor_id == agent_id)
                .map(|pressure| format!("inclinado a {}", pressure.agenda_tag))
                .unwrap_or_else(|| "sem alinhamento politico forte".to_string())
        } else {
            positions.truncate(3);
            positions.join(" | ")
        }
    }

    pub(super) fn political_grievances_for_agent(&self, agent_id: u64) -> Vec<String> {
        self.political_pressures
            .iter()
            .filter(|pressure| pressure.actor_id == agent_id)
            .take(4)
            .map(|pressure| {
                format!(
                    "{}:{} intensidade={} ({})",
                    pressure.domain.as_str(),
                    pressure.agenda_tag,
                    pressure.intensity,
                    pressure.reason
                )
            })
            .collect()
    }

    pub(super) fn political_influence(&mut self, agent_id: u64) -> i32 {
        let mut query = self
            .world
            .query::<(&AgentCore, &RelationComponent, &LifeStatusComponent)>();
        let Some((role_id, relations, life_status, household_id)) = query
            .iter(&self.world)
            .find_map(|(core, relations, life_status)| {
                (core.id == agent_id).then(|| {
                    (
                        core.role_id.clone(),
                        relations.0.clone(),
                        life_status.0,
                        core.home_building_id,
                    )
                })
            })
        else {
            return 0;
        };
        if life_status != AgentLifeStatus::Vivo {
            return 0;
        }
        let mut influence = 10;
        if role_id == Role::Headman.id() {
            influence += 15;
        } else if role_id == Role::Guard.id() {
            influence += 8;
        }
        let relation_reputation = if relations.is_empty() {
            0
        } else {
            relations
                .values()
                .map(|relation| relation.reputation + relation.trust / 3 - relation.resentment / 3)
                .sum::<i32>()
                / relations.len() as i32
        };
        influence += relation_reputation.clamp(-8, 12);
        if let Some(household_id) = household_id
            && let Some(household) = self.household_by_id(household_id)
        {
            influence += (household.treasury / 12).clamp(0, 10);
            influence -= (household.tax_arrears / 2).clamp(0, 8);
            influence -= (household.feudal_arrears / 2).clamp(0, 6);
        }
        influence += (self.feudal_power_for_agent(agent_id) / 12).clamp(0, 18);
        if self.crime_cases.iter().any(|case| {
            case.suspect_id == Some(agent_id) && !matches!(case.status, CrimeCaseStatus::Closed)
        }) {
            influence -= 8;
        }
        influence.clamp(0, 45)
    }

    pub(super) fn preferred_political_issue_for_actor(
        &mut self,
        actor_id: u64,
    ) -> Option<PoliticalIssueId> {
        if let Some(pressure) = self
            .political_pressures
            .iter()
            .find(|pressure| pressure.actor_id == actor_id)
            && let Some(issue) = self.political_issues.iter().find(|issue| {
                issue.status == PoliticalIssueStatus::Open
                    && issue.domain == pressure.domain
                    && issue.proposed_value == pressure.proposed_value
                    && issue.agenda_tag == pressure.agenda_tag
            })
        {
            return Some(issue.id);
        }
        self.political_issues
            .iter()
            .filter(|issue| issue.status == PoliticalIssueStatus::Open)
            .max_by_key(|issue| issue.support_score - issue.opposition_score)
            .map(|issue| issue.id)
    }

    pub(super) fn record_political_position(
        &mut self,
        actor_id: u64,
        issue_id: PoliticalIssueId,
        support: bool,
    ) -> Result<bool> {
        let influence = self.political_influence(actor_id).max(1);
        let Some(issue) = self
            .political_issues
            .iter_mut()
            .find(|issue| issue.id == issue_id && issue.status == PoliticalIssueStatus::Open)
        else {
            return Ok(false);
        };
        let mut changed = false;
        if support {
            if !issue.supporter_ids.contains(&actor_id) {
                issue.supporter_ids.push(actor_id);
                issue.support_score += influence;
                changed = true;
            }
            issue.opposer_ids.retain(|id| *id != actor_id);
        } else {
            if !issue.opposer_ids.contains(&actor_id) {
                issue.opposer_ids.push(actor_id);
                issue.opposition_score += influence;
                changed = true;
            }
            issue.supporter_ids.retain(|id| *id != actor_id);
        }
        Ok(changed)
    }

    pub(super) fn refresh_political_state(&mut self) -> Result<()> {
        self.refresh_feudal_state()?;
        self.political_pressures = self.derive_political_pressures();
        self.ensure_political_issues_and_factions()?;
        self.check_faction_founding()?;
        self.update_faction_rage_and_activity()?;
        self.update_insurrections()?;
        self.check_faction_resolution()?;
        Ok(())
    }

    pub(super) fn refresh_feudal_state(&mut self) -> Result<()> {
        self.apply_daily_feudal_obligations()?;
        self.refresh_power_centers();
        self.refresh_succession_crises()?;
        Ok(())
    }

    pub(super) fn apply_daily_feudal_obligations(&mut self) -> Result<()> {
        if self.tick_of_day + 1 != self.ticks_per_day {
            return Ok(());
        }
        let households = self.households.clone();
        let mut events = Vec::new();
        for household in households {
            let Some(lord_id) = household.direct_lord_agent_id else {
                continue;
            };
            let due = household.feudal_tribute_due.max(0);
            if due <= 0 {
                continue;
            }
            let payment = household.treasury.min(due);
            if let Some(entry) = self
                .households
                .iter_mut()
                .find(|entry| entry.id == household.id)
            {
                entry.treasury -= payment;
                let unpaid = due - payment;
                if unpaid > 0 {
                    entry.feudal_arrears += unpaid;
                } else {
                    entry.feudal_arrears = (entry.feudal_arrears - 1).max(0);
                }
                entry.corvee_days_due = (entry.corvee_days_due + 1).clamp(0, 12);
            }
            if let Some(lord_household_id) = self
                .agent_core_snapshot(lord_id)
                .and_then(|core| core.home_building_id)
            {
                if let Some(lord_household) = self
                    .households
                    .iter_mut()
                    .find(|entry| entry.id == lord_household_id)
                {
                    lord_household.treasury += payment;
                }
            }
            if payment > 0 {
                events.push(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: household.member_ids.first().copied().unwrap_or(0),
                    target: Some(lord_id),
                    kind: EventKind::TributePaid,
                    summary: format!(
                        "{} pagou {} moeda(s) de tributo feudal a {}.",
                        household.name,
                        payment,
                        self.agent_name(lord_id)
                            .unwrap_or_else(|_| format!("agente {}", lord_id))
                    ),
                    impact_tags: vec!["feudal".to_string(), "tributo".to_string()],
                });
            }
            if payment < due {
                events.push(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: household.member_ids.first().copied().unwrap_or(0),
                    target: Some(lord_id),
                    kind: EventKind::TributeRefused,
                    summary: format!(
                        "{} nao conseguiu cumprir tributo feudal completo: devia {}, pagou {}.",
                        household.name, due, payment
                    ),
                    impact_tags: vec![
                        "feudal".to_string(),
                        "inadimplencia".to_string(),
                        "tributo".to_string(),
                    ],
                });
            }
        }
        self.events.extend(events);
        Ok(())
    }

    pub(super) fn refresh_power_centers(&mut self) {
        for center in &mut self.power_centers {
            let title = center
                .title_id
                .and_then(|title_id| self.feudal_titles.iter().find(|title| title.id == title_id));
            let material = center
                .title_id
                .and_then(|title_id| {
                    self.feudal_titles
                        .iter()
                        .find(|title| title.id == title_id)
                        .and_then(|title| title.holding_id)
                })
                .and_then(|holding_id| {
                    self.estate_holdings
                        .iter()
                        .find(|holding| holding.id == holding_id)
                        .map(|holding| holding.annualized_value / 12)
                })
                .unwrap_or(center.material_power);
            center.material_power = material;
            if let Some(title) = title {
                center.formal_authority = title.precedence;
                center.legitimacy = title.legitimacy;
                center.summary = format!(
                    "{} | autoridade formal={} poder material={} coerção={}",
                    title.name,
                    center.formal_authority,
                    center.material_power,
                    center.coercive_power
                );
            }
            center.stability =
                (center.formal_authority + center.legitimacy + center.material_power) / 3;
        }
    }

    pub(super) fn refresh_succession_crises(&mut self) -> Result<()> {
        let titles = self.feudal_titles.clone();
        for title in titles {
            if !title.active {
                continue;
            }
            let holder_dead = title
                .holder_agent_id
                .and_then(|holder_id| self.life_status(holder_id).ok())
                .is_some_and(|status| status != AgentLifeStatus::Vivo);
            if holder_dead
                && !self.succession_crises.iter().any(|crisis| {
                    crisis.title_id == title.id && crisis.status == SuccessionCrisisStatus::Open
                })
            {
                self.open_succession_crisis_for_title(&title)?;
            }
        }
        self.resolve_stable_succession_crises()?;
        Ok(())
    }

    pub(super) fn resolve_stable_succession_crises(&mut self) -> Result<()> {
        let open_crises = self
            .succession_crises
            .iter()
            .filter(|crisis| crisis.status == SuccessionCrisisStatus::Open)
            .cloned()
            .collect::<Vec<_>>();
        for crisis in open_crises {
            let age = self.day.saturating_sub(crisis.opened_day);
            let Some(title) = self
                .feudal_titles
                .iter()
                .find(|title| title.id == crisis.title_id)
                .cloned()
            else {
                continue;
            };
            let best_claimant = crisis
                .claimant_ids
                .iter()
                .copied()
                .max_by_key(|claimant_id| {
                    self.feudal_power_for_agent(*claimant_id)
                        + if Some(*claimant_id) == crisis.recognized_heir_id {
                            12
                        } else {
                            0
                        }
                        + if Some(*claimant_id) == crisis.usurper_id {
                            6
                        } else {
                            0
                        }
                });
            let should_resolve_regular = best_claimant.is_some()
                && (crisis.claimant_ids.len() <= 1
                    || crisis.conflict_score <= 20
                    || (age >= 2 && crisis.legitimacy_gap <= 18));
            let should_resolve_by_force = crisis.usurper_id.is_some()
                && age >= 4
                && crisis.conflict_score >= 50;
            if !(should_resolve_regular || should_resolve_by_force) {
                continue;
            }
            let successor_id = if should_resolve_by_force {
                crisis.usurper_id.or(best_claimant)
            } else {
                crisis.recognized_heir_id.or(best_claimant)
            };
            let Some(successor_id) = successor_id else {
                continue;
            };
            let successor_name = self
                .agent_name(successor_id)
                .unwrap_or_else(|_| format!("agente {}", successor_id));
            if let Some(title_mut) = self
                .feudal_titles
                .iter_mut()
                .find(|entry| entry.id == crisis.title_id)
            {
                title_mut.holder_agent_id = Some(successor_id);
                title_mut.legitimacy = if should_resolve_by_force {
                    (title.legitimacy - 10).clamp(-100, 100)
                } else {
                    (title.legitimacy + 6).clamp(-100, 100)
                };
                title_mut.active = true;
            }
            if let Some(crisis_mut) = self
                .succession_crises
                .iter_mut()
                .find(|entry| entry.id == crisis.id)
            {
                crisis_mut.status = SuccessionCrisisStatus::Resolved;
                crisis_mut.recognized_heir_id = Some(successor_id);
                crisis_mut.resolved_day = Some(self.day);
                crisis_mut.summary = if should_resolve_by_force {
                    format!(
                        "Crise do titulo {} terminou em usurpacao por {}.",
                        title.name, successor_name
                    )
                } else {
                    format!(
                        "Crise do titulo {} terminou com reconhecimento de {}.",
                        title.name, successor_name
                    )
                };
            }
            if let Some(territory_id) = title.territory_id
                && let Some(territory) = self
                    .territories
                    .iter_mut()
                    .find(|territory| territory.id == territory_id)
            {
                territory.stability = if should_resolve_by_force {
                    (territory.stability - 8).clamp(0, 100)
                } else {
                    (territory.stability + 5).clamp(0, 100)
                };
            }
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: successor_id,
                target: None,
                kind: if should_resolve_by_force {
                    EventKind::Usurpation
                } else {
                    EventKind::SuccessionRecognized
                },
                summary: if should_resolve_by_force {
                    format!(
                        "{} tomou o titulo {} ao encerrar a crise sucessoria.",
                        successor_name, title.name
                    )
                } else {
                    format!(
                        "{} foi reconhecido como sucessor de {}.",
                        successor_name, title.name
                    )
                },
                impact_tags: vec![
                    "feudal".to_string(),
                    "sucessao".to_string(),
                    if should_resolve_by_force {
                        "usurpacao".to_string()
                    } else {
                        "reconhecimento".to_string()
                    },
                ],
            });
        }
        Ok(())
    }

    pub(super) fn open_succession_crisis_for_title(&mut self, title: &FeudalTitle) -> Result<()> {
        let mut claimants = Vec::new();
        if let Some(holder_id) = title.holder_agent_id {
            if let Some(lineage) = self.lineage_snapshot(holder_id) {
                for child_id in lineage.children {
                    if self.life_status(child_id).unwrap_or(AgentLifeStatus::Morto)
                        == AgentLifeStatus::Vivo
                    {
                        claimants.push(child_id);
                    }
                }
                if claimants.is_empty() {
                    if let Some(spouse_id) = lineage.spouse {
                        if self
                            .life_status(spouse_id)
                            .unwrap_or(AgentLifeStatus::Morto)
                            == AgentLifeStatus::Vivo
                        {
                            claimants.push(spouse_id);
                        }
                    }
                }
            }
            let office_claimants = self
                .authority_offices
                .iter()
                .filter(|office| office.active && office.title_id == Some(title.id))
                .filter_map(|office| office.holder_agent_id)
                .collect::<Vec<_>>();
            claimants.extend(office_claimants);
        }
        if claimants.is_empty() {
            if let Some(suzerain_title_id) = title.suzerain_title_id {
                let suzerain_holder = self
                    .feudal_titles
                    .iter()
                    .find(|entry| entry.id == suzerain_title_id)
                    .and_then(|entry| entry.holder_agent_id);
                if let Some(suzerain_holder) = suzerain_holder {
                    claimants.push(suzerain_holder);
                }
            }
        }
        claimants.sort_unstable();
        claimants.dedup();
        let recognized_heir_id = claimants.first().copied();
        let crisis_id = self.next_succession_crisis_id;
        self.next_succession_crisis_id += 1;
        self.succession_crises.push(SuccessionCrisis {
            id: crisis_id,
            title_id: title.id,
            territory_id: title.territory_id,
            claimant_ids: claimants.clone(),
            recognized_heir_id,
            usurper_id: None,
            status: SuccessionCrisisStatus::Open,
            legitimacy_gap: if claimants.len() <= 1 { 10 } else { 35 },
            conflict_score: if claimants.len() <= 1 { 15 } else { 45 },
            opened_day: self.day,
            resolved_day: None,
            summary: format!(
                "Vacancia do titulo {} abriu disputa entre {:?}.",
                title.name, claimants
            ),
        });
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: title.holder_agent_id.unwrap_or(0),
            target: recognized_heir_id,
            kind: EventKind::SuccessionOpened,
            summary: format!(
                "Crise sucessoria aberta para {} com pretendentes {:?}.",
                title.name, claimants
            ),
            impact_tags: vec!["feudal".to_string(), "sucessao".to_string()],
        });
        Ok(())
    }

    pub(super) fn derive_political_pressures(&mut self) -> Vec<PoliticalPressure> {
        let mut pressures = Vec::new();
        let households = self.households.clone();
        for household in &households {
            for agent_id in &household.member_ids {
                if household.tax_arrears > 0 {
                    pressures.push(PoliticalPressure {
                        actor_id: *agent_id,
                        household_id: Some(household.id),
                        agenda_tag: "reduzir_imposto".to_string(),
                        domain: PolicyDomain::Tax,
                        proposed_value: "reduzir".to_string(),
                        intensity: (household.tax_arrears
                            + self.village_economy.daily_household_tax)
                            .clamp(1, 20),
                        reason: format!(
                            "{} deve {} moeda(s) de imposto",
                            household.name, household.tax_arrears
                        ),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
                if household.food_crisis_level > 0 {
                    pressures.push(PoliticalPressure {
                        actor_id: *agent_id,
                        household_id: Some(household.id),
                        agenda_tag: "priorizar_lares_na_comida".to_string(),
                        domain: PolicyDomain::Rationing,
                        proposed_value: RationingPolicy::HouseholdFirst.as_str().to_string(),
                        intensity: i32::from(household.food_crisis_level) * 8
                            + household.scarcity_pressure.clamp(0, 12),
                        reason: format!(
                            "{} esta em crise alimentar nivel {}",
                            household.name, household.food_crisis_level
                        ),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
                if household.feudal_arrears > 0 {
                    pressures.push(PoliticalPressure {
                        actor_id: *agent_id,
                        household_id: Some(household.id),
                        agenda_tag: "boicote_tributo".to_string(),
                        domain: PolicyDomain::Tax,
                        proposed_value: "resistir_tributo".to_string(),
                        intensity: household.feudal_arrears.clamp(2, 25),
                        reason: format!(
                            "{} acumula atrasos feudais de {} moeda(s)",
                            household.name, household.feudal_arrears
                        ),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
                if household.corvee_days_due >= 3 {
                    pressures.push(PoliticalPressure {
                        actor_id: *agent_id,
                        household_id: Some(household.id),
                        agenda_tag: "anti_corveia".to_string(),
                        domain: PolicyDomain::Justice,
                        proposed_value: "reduzir_corveia".to_string(),
                        intensity: household.corvee_days_due.clamp(3, 20),
                        reason: format!(
                            "{} sofre carga de corveia de {} dia(s)",
                            household.name, household.corvee_days_due
                        ),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
            }
        }
        let open_succession_crises = self
            .succession_crises
            .iter()
            .filter(|crisis| crisis.status == SuccessionCrisisStatus::Open)
            .cloned()
            .collect::<Vec<_>>();
        for crisis in open_succession_crises {
            for claimant_id in &crisis.claimant_ids {
                pressures.push(PoliticalPressure {
                    actor_id: *claimant_id,
                    household_id: self.household_id_for_agent_immutable(*claimant_id),
                    agenda_tag: "crise_sucessoria".to_string(),
                    domain: PolicyDomain::Justice,
                    proposed_value: "reconhecer_herdeiro".to_string(),
                    intensity: crisis.conflict_score.clamp(8, 35),
                    reason: format!("pretensao aberta em {}", crisis.summary),
                    day: self.day,
                    tick: self.tick_of_day,
                });
            }
        }

        let crime_cases = self.crime_cases.clone();
        for case in &crime_cases {
            if matches!(
                case.status,
                CrimeCaseStatus::Open | CrimeCaseStatus::Investigating | CrimeCaseStatus::Proven
            ) && case.severity >= 50
            {
                if let Some(victim_id) = case.victim_id {
                    pressures.push(PoliticalPressure {
                        actor_id: victim_id,
                        household_id: self.household_id_for_agent_immutable(victim_id),
                        agenda_tag: "endurecer_justica".to_string(),
                        domain: PolicyDomain::Justice,
                        proposed_value: JusticeSeverity::Severe.as_str().to_string(),
                        intensity: i32::from(case.severity / 8).clamp(1, 20),
                        reason: format!("caso criminal {} sem resposta suficiente", case.id),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
            }
            if case.status == CrimeCaseStatus::Punished
                && let Some(suspect_id) = case.suspect_id
            {
                pressures.push(PoliticalPressure {
                    actor_id: suspect_id,
                    household_id: self.household_id_for_agent_immutable(suspect_id),
                    agenda_tag: "abrandar_justica".to_string(),
                    domain: PolicyDomain::Justice,
                    proposed_value: JusticeSeverity::Lenient.as_str().to_string(),
                    intensity: i32::from(case.severity / 10).clamp(1, 16),
                    reason: format!("punicao no caso {} gera ressentimento legal", case.id),
                    day: self.day,
                    tick: self.tick_of_day,
                });
            }
        }

        if self.village_economy.public_treasury < 20 {
            for (agent_id, role_id) in self.agent_role_pairs() {
                if role_id == Role::Guard.id() || role_id == Role::Headman.id() {
                    pressures.push(PoliticalPressure {
                        actor_id: agent_id,
                        household_id: self.household_id_for_agent_immutable(agent_id),
                        agenda_tag: "aumentar_imposto".to_string(),
                        domain: PolicyDomain::Tax,
                        proposed_value: "aumentar".to_string(),
                        intensity: (20 - self.village_economy.public_treasury).clamp(1, 20),
                        reason: "caixa publico baixo ameaca servico civico".to_string(),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
            }
        }
        for agent_id in self.agent_ids() {
            let Some(perception) = self.institutional_perception(agent_id) else {
                continue;
            };
            let household_id = self.household_id_for_agent_immutable(agent_id);
            if perception.tax_legitimacy <= -25 {
                pressures.push(PoliticalPressure {
                    actor_id: agent_id,
                    household_id,
                    agenda_tag: "boicote_imposto".to_string(),
                    domain: PolicyDomain::Tax,
                    proposed_value: "reduzir".to_string(),
                    intensity: (-perception.tax_legitimacy / 4
                        + perception.perceived_corruption.max(0) / 8)
                        .clamp(1, 25),
                    reason: "baixa legitimidade percebida do imposto".to_string(),
                    day: self.day,
                    tick: self.tick_of_day,
                });
            }
            if perception.rationing_legitimacy <= -25 {
                let hunger = self
                    .agent_state(agent_id)
                    .map(|state| state.hunger)
                    .unwrap_or(0);
                pressures.push(PoliticalPressure {
                    actor_id: agent_id,
                    household_id,
                    agenda_tag: "motim_comida".to_string(),
                    domain: PolicyDomain::Rationing,
                    proposed_value: RationingPolicy::HouseholdFirst.as_str().to_string(),
                    intensity: (-perception.rationing_legitimacy / 5 + hunger / 10).clamp(1, 25),
                    reason: "racionamento percebido como injusto".to_string(),
                    day: self.day,
                    tick: self.tick_of_day,
                });
            }
            if perception.justice_legitimacy <= -30 {
                pressures.push(PoliticalPressure {
                    actor_id: agent_id,
                    household_id,
                    agenda_tag: "justica_vigilante".to_string(),
                    domain: PolicyDomain::Justice,
                    proposed_value: JusticeSeverity::Severe.as_str().to_string(),
                    intensity: (-perception.justice_legitimacy / 4
                        + perception.perceived_corruption.max(0) / 10)
                        .clamp(1, 25),
                    reason: "justica local percebida como ilegitima".to_string(),
                    day: self.day,
                    tick: self.tick_of_day,
                });
            }
            if perception.leader_legitimacy <= -35 || perception.war_support <= -35 {
                pressures.push(PoliticalPressure {
                    actor_id: agent_id,
                    household_id,
                    agenda_tag: "depor_lider".to_string(),
                    domain: PolicyDomain::Justice,
                    proposed_value: "normal".to_string(),
                    intensity: ((-perception.leader_legitimacy).max(-perception.war_support) / 4
                        + perception.fear_of_authority.max(0) / 12)
                        .clamp(1, 30),
                    reason: "lideranca perdeu legitimidade institucional".to_string(),
                    day: self.day,
                    tick: self.tick_of_day,
                });
            }
        }
        let rumors = self.rumors.clone();
        for rumor in rumors
            .iter()
            .filter(|rumor| !rumor.is_disproven && !rumor.is_confirmed)
        {
            let topic = rumor.topic.to_lowercase();
            let claim = rumor.claim.to_lowercase();
            for agent_id in rumor.known_by.clone() {
                let belief = self
                    .rumor_beliefs(agent_id)
                    .into_iter()
                    .find(|belief| belief.rumor_id == rumor.id)
                    .map(|belief| belief.belief)
                    .unwrap_or(0);
                if belief < 50 {
                    continue;
                }
                let household_id = self.household_id_for_agent_immutable(agent_id);
                if topic.contains("corrup") || claim.contains("corrup") || claim.contains("desvio")
                {
                    pressures.push(PoliticalPressure {
                        actor_id: agent_id,
                        household_id,
                        agenda_tag: "depor_lider".to_string(),
                        domain: PolicyDomain::Justice,
                        proposed_value: "normal".to_string(),
                        intensity: (belief / 5 + rumor.distortion / 12).clamp(1, 25),
                        reason: format!("rumor crivel de corrupcao #{}", rumor.id),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
                if topic.contains("comida")
                    || topic.contains("fome")
                    || topic.contains("escassez")
                    || claim.contains("fome")
                    || claim.contains("graos")
                {
                    pressures.push(PoliticalPressure {
                        actor_id: agent_id,
                        household_id,
                        agenda_tag: "motim_comida".to_string(),
                        domain: PolicyDomain::Rationing,
                        proposed_value: RationingPolicy::HouseholdFirst.as_str().to_string(),
                        intensity: (belief / 6).clamp(1, 20),
                        reason: format!("rumor de escassez alimentar #{}", rumor.id),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
                if topic.contains("guerra")
                    || topic.contains("derrota")
                    || claim.contains("invasao")
                    || claim.contains("derrota")
                {
                    pressures.push(PoliticalPressure {
                        actor_id: agent_id,
                        household_id,
                        agenda_tag: "boicote_imposto".to_string(),
                        domain: PolicyDomain::Tax,
                        proposed_value: "reduzir".to_string(),
                        intensity: (belief / 7).clamp(1, 18),
                        reason: format!("rumor de guerra/derrota #{}", rumor.id),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
                if rumor.is_slander && belief >= 60 {
                    pressures.push(PoliticalPressure {
                        actor_id: agent_id,
                        household_id,
                        agenda_tag: "justica_vigilante".to_string(),
                        domain: PolicyDomain::Justice,
                        proposed_value: JusticeSeverity::Severe.as_str().to_string(),
                        intensity: (belief / 7).clamp(1, 18),
                        reason: format!("rumor hostil nao confirmado #{}", rumor.id),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
            }
        }
        let active_wars = self
            .wars
            .iter()
            .filter(|war| war.status == WarStatus::Active)
            .cloned()
            .collect::<Vec<_>>();
        if !active_wars.is_empty() {
            let agent_ids = self.agent_ids();
            for war in active_wars {
                let (_, stress_inc, hunger_inc, _, _) = Self::war_stage_impact(war.stage);
                for agent_id in &agent_ids {
                    let household_id = self.household_id_for_agent_immutable(*agent_id);
                    if hunger_inc > 0 {
                        pressures.push(PoliticalPressure {
                            actor_id: *agent_id,
                            household_id,
                            agenda_tag: "motim_comida".to_string(),
                            domain: PolicyDomain::Rationing,
                            proposed_value: RationingPolicy::HouseholdFirst.as_str().to_string(),
                            intensity: (hunger_inc + stress_inc / 2).clamp(1, 30),
                            reason: format!("fome e medo causados pela guerra #{}", war.id),
                            day: self.day,
                            tick: self.tick_of_day,
                        });
                    }
                    pressures.push(PoliticalPressure {
                        actor_id: *agent_id,
                        household_id,
                        agenda_tag: "boicote_imposto".to_string(),
                        domain: PolicyDomain::Tax,
                        proposed_value: "reduzir".to_string(),
                        intensity: (stress_inc / 2 + self.village_economy.daily_household_tax)
                            .clamp(1, 25),
                        reason: format!("custo militar da guerra #{}", war.id),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                    if matches!(war.stage, WarStage::DecisiveBattle | WarStage::Occupation) {
                        pressures.push(PoliticalPressure {
                            actor_id: *agent_id,
                            household_id,
                            agenda_tag: "depor_lider".to_string(),
                            domain: PolicyDomain::Justice,
                            proposed_value: "normal".to_string(),
                            intensity: stress_inc.clamp(1, 30),
                            reason: format!("perdas e medo da guerra #{}", war.id),
                            day: self.day,
                            tick: self.tick_of_day,
                        });
                    }
                    if war.stage == WarStage::Occupation {
                        pressures.push(PoliticalPressure {
                            actor_id: *agent_id,
                            household_id,
                            agenda_tag: "resistencia_ocupacao".to_string(),
                            domain: PolicyDomain::Justice,
                            proposed_value: "resistir".to_string(),
                            intensity: 25,
                            reason: format!("ocupacao/invasao na guerra #{}", war.id),
                            day: self.day,
                            tick: self.tick_of_day,
                        });
                    }
                }
            }
        }
        let recent_failed_demands = self
            .military_demands
            .iter()
            .filter(|demand| {
                demand.status == MilitaryDemandStatus::Failed
                    && self.day.saturating_sub(demand.deadline_day) <= 1
            })
            .cloned()
            .collect::<Vec<_>>();
        if !recent_failed_demands.is_empty() {
            let agent_ids = self.agent_ids();
            for demand in recent_failed_demands {
                let food_shortage = Self::missing_military_resources_for_demand(&demand)
                    .into_iter()
                    .filter(|stack| self.is_food_resource(&stack.resource_id))
                    .map(|stack| stack.amount)
                    .sum::<i32>();
                let cash_shortage = (demand.cash_required - demand.cash_delivered).max(0);
                let (agenda_tag, domain, proposed_value) = if food_shortage > 0 {
                    (
                        "motim_comida",
                        PolicyDomain::Rationing,
                        RationingPolicy::HouseholdFirst.as_str().to_string(),
                    )
                } else if cash_shortage > 0 {
                    ("boicote_imposto", PolicyDomain::Tax, "reduzir".to_string())
                } else {
                    ("depor_lider", PolicyDomain::Justice, "normal".to_string())
                };
                for agent_id in agent_ids.iter().take(4) {
                    pressures.push(PoliticalPressure {
                        actor_id: *agent_id,
                        household_id: self.household_id_for_agent_immutable(*agent_id),
                        agenda_tag: agenda_tag.to_string(),
                        domain,
                        proposed_value: proposed_value.clone(),
                        intensity: (demand.shortage_score / 2 + 8).clamp(5, 40),
                        reason: format!(
                            "demanda militar #{} falhou durante a guerra #{}",
                            demand.id, demand.war_id
                        ),
                        day: self.day,
                        tick: self.tick_of_day,
                    });
                }
            }
        }
        pressures
    }

    pub(super) fn ensure_political_issues_and_factions(&mut self) -> Result<()> {
        let mut grouped: HashMap<(PolicyDomain, String, String), Vec<PoliticalPressure>> =
            HashMap::new();
        for pressure in self.political_pressures.clone() {
            grouped
                .entry((
                    pressure.domain,
                    pressure.proposed_value.clone(),
                    pressure.agenda_tag.clone(),
                ))
                .or_default()
                .push(pressure);
        }

        for ((domain, proposed_value, agenda_tag), pressures) in grouped {
            let mut member_ids = pressures
                .iter()
                .map(|pressure| pressure.actor_id)
                .collect::<Vec<_>>();
            member_ids.sort_unstable();
            member_ids.dedup();
            let influence = member_ids
                .iter()
                .map(|agent_id| self.political_influence(*agent_id))
                .sum::<i32>();
            if member_ids.len() < 2 && influence < 25 {
                continue;
            }

            let issue_id = if let Some(issue) = self.political_issues.iter().find(|issue| {
                issue.status == PoliticalIssueStatus::Open
                    && issue.domain == domain
                    && issue.proposed_value == proposed_value
                    && issue.agenda_tag == agenda_tag
            }) {
                issue.id
            } else {
                let issue_id = self.next_political_issue_id;
                self.next_political_issue_id += 1;
                let summary = political_issue_summary(domain, &proposed_value, &agenda_tag);
                self.political_issues.push(PoliticalIssue {
                    id: issue_id,
                    agenda_tag: agenda_tag.clone(),
                    domain,
                    proposed_value: proposed_value.clone(),
                    summary: summary.clone(),
                    proposed_by: member_ids.first().copied(),
                    support_score: influence / 2,
                    opposition_score: 0,
                    supporter_ids: member_ids.clone(),
                    opposer_ids: Vec::new(),
                    status: PoliticalIssueStatus::Open,
                    opened_day: self.day,
                    resolved_day: None,
                });
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: member_ids.first().copied().unwrap_or(0),
                    target: None,
                    kind: EventKind::PolicyProposal,
                    summary: format!("Nova pauta politica: {summary}."),
                    impact_tags: vec!["politica".to_string(), agenda_tag.clone()],
                });
                issue_id
            };

            if let Some(faction) = self
                .political_factions
                .iter_mut()
                .find(|faction| faction.agenda_tag == agenda_tag)
            {
                if !faction.support_issue_ids.contains(&issue_id) {
                    faction.support_issue_ids.push(issue_id);
                }
            }
        }
        Ok(())
    }

    pub(super) fn resolve_daily_politics(&mut self) -> Result<()> {
        self.refresh_political_state()?;
        let open_issue_ids = self
            .political_issues
            .iter()
            .filter(|issue| issue.status == PoliticalIssueStatus::Open)
            .map(|issue| issue.id)
            .collect::<Vec<_>>();
        for issue_id in open_issue_ids {
            let Some(issue_index) = self
                .political_issues
                .iter()
                .position(|issue| issue.id == issue_id)
            else {
                continue;
            };
            let supporting_pressure_actor_ids = self
                .political_pressures
                .iter()
                .filter(|pressure| {
                    let issue = &self.political_issues[issue_index];
                    pressure.domain == issue.domain
                        && pressure.proposed_value == issue.proposed_value
                        && pressure.agenda_tag == issue.agenda_tag
                })
                .map(|pressure| pressure.actor_id)
                .collect::<Vec<_>>();
            let pressure_support = supporting_pressure_actor_ids
                .into_iter()
                .map(|actor_id| self.political_influence(actor_id) / 2)
                .sum::<i32>();
            let issue = &self.political_issues[issue_index];
            let support = issue.support_score + pressure_support;
            let opposition = issue.opposition_score;
            let net = support - opposition;
            let passed = net >= 25;
            let summary = issue.summary.clone();
            let actor = {
                let issue = &mut self.political_issues[issue_index];
                issue.support_score = support;
                issue.opposition_score = opposition;
                issue.status = if passed {
                    PoliticalIssueStatus::Passed
                } else {
                    PoliticalIssueStatus::Rejected
                };
                issue.resolved_day = Some(self.day);
                issue.proposed_by.unwrap_or(0)
            };
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor,
                target: None,
                kind: EventKind::InstitutionalDispute,
                summary: if passed {
                    format!(
                        "Pauta popular ganhou forca, mas nao muda norma sem decreto: {summary} (saldo politico {net})."
                    )
                } else {
                    format!("Pauta rejeitada: {summary} (saldo politico {net}).")
                },
                impact_tags: vec!["politica".to_string(), "disputa".to_string()],
            });
            if passed {
                for faction in self
                    .political_factions
                    .iter_mut()
                    .filter(|faction| faction.support_issue_ids.contains(&issue_id))
                {
                    faction.rage = (faction.rage + 15).clamp(0, 100);
                    if faction.rage >= 50 && faction.member_ids.len() >= 2 {
                        faction.is_action_active = true;
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn update_abstract_wars(&mut self) -> Result<()> {
        self.settle_due_military_demands()?;
        let active_wars = self
            .wars
            .iter()
            .filter(|war| war.status == WarStatus::Active)
            .cloned()
            .collect::<Vec<_>>();
        for war in &active_wars {
            self.apply_daily_war_impacts(war)?;
        }

        let polity_power = self
            .polities
            .iter()
            .map(|polity| {
                let controlled_value = self
                    .territories
                    .iter()
                    .filter(|territory| territory.controller_polity_id == polity.id)
                    .map(|territory| territory.strategic_value)
                    .sum::<i32>();
                (
                    polity.id,
                    polity.military_readiness
                        + (polity.treasury / 10)
                        + controlled_value
                        + self.recent_military_supply_score_for_polity(polity.id)
                        + self.feudal_war_support_for_polity(polity.id),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut events = Vec::new();
        let mut territorial_transfers = Vec::new();
        for war in &mut self.wars {
            if war.status != WarStatus::Active {
                continue;
            }
            let attacker_power = *polity_power.get(&war.attacker_polity_id).unwrap_or(&10);
            let defender_power = *polity_power.get(&war.defender_polity_id).unwrap_or(&10);
            let attacker_gain = (10 + (attacker_power - defender_power).max(0) / 10).clamp(5, 25);
            let defender_gain = (10 + (defender_power - attacker_power).max(0) / 10).clamp(5, 25);
            war.attacker_score = (war.attacker_score + attacker_gain).clamp(0, 100);
            war.defender_score = (war.defender_score + defender_gain).clamp(0, 100);
            let leading_score = war.attacker_score.max(war.defender_score);
            war.stage = match leading_score {
                0..=24 => WarStage::Mobilization,
                25..=49 => WarStage::Raids,
                50..=74 => WarStage::Siege,
                75..=99 => WarStage::DecisiveBattle,
                _ => WarStage::Occupation,
            };
            events.push(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: 0,
                target: None,
                kind: EventKind::InstitutionalDispute,
                summary: format!(
                    "Guerra #{} avancou para {:?}: atacante={} defensor={}.",
                    war.id, war.stage, war.attacker_score, war.defender_score
                ),
                impact_tags: vec!["guerra".to_string(), format!("war:{}", war.id)],
            });
            if leading_score >= 100 {
                let winner = if war.attacker_score >= war.defender_score {
                    war.attacker_polity_id
                } else {
                    war.defender_polity_id
                };
                war.status = WarStatus::Won;
                war.winner_polity_id = Some(winner);
                war.ended_day = Some(self.day);
                territorial_transfers.extend(
                    war.target_territory_ids
                        .iter()
                        .copied()
                        .map(|territory_id| (territory_id, winner, war.id)),
                );
                events.push(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: 0,
                    target: None,
                    kind: EventKind::InstitutionalDispute,
                    summary: format!(
                        "Guerra #{} terminou: polity {} venceu automaticamente ao atingir 100 pontos.",
                        war.id, winner
                    ),
                    impact_tags: vec!["guerra".to_string(), "vitoria".to_string()],
                });
            }
        }
        for (territory_id, winner, war_id) in territorial_transfers {
            if let Some(territory) = self
                .territories
                .iter_mut()
                .find(|territory| territory.id == territory_id)
            {
                let old_controller = territory.controller_polity_id;
                territory.controller_polity_id = winner;
                territory.stability = 45;
                if !territory.claimed_by.contains(&winner) {
                    territory.claimed_by.push(winner);
                }
                events.push(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: 0,
                    target: None,
                    kind: EventKind::NormChanged,
                    summary: format!(
                        "Controle territorial de {} mudou de polity {} para polity {} apos a guerra #{}.",
                        territory.name, old_controller, winner, war_id
                    ),
                    impact_tags: vec![
                        "territorio".to_string(),
                        "controle".to_string(),
                        "guerra".to_string(),
                        format!("territory:{}", territory.id),
                    ],
                });
            }
        }
        self.events.extend(events);
        self.ensure_daily_military_demands_for_active_wars();
        Ok(())
    }

    pub(super) fn recent_military_supply_score_for_polity(&self, polity_id: PolityId) -> i32 {
        let recent = self
            .military_demands
            .iter()
            .filter(|demand| {
                demand.polity_id == polity_id
                    && demand.deadline_day <= self.day
                    && self.day.saturating_sub(demand.deadline_day) <= 2
            })
            .collect::<Vec<_>>();
        if recent.is_empty() {
            return 0;
        }
        let total = recent
            .iter()
            .map(|demand| {
                let required_units: i32 = demand.required.iter().map(|stack| stack.amount).sum();
                let delivered_units: i32 = demand.delivered.iter().map(|stack| stack.amount).sum();
                let required = required_units + demand.cash_required;
                if required <= 0 {
                    0
                } else {
                    let delivered = delivered_units + demand.cash_delivered;
                    ((delivered * 100 / required) - 60) / 2
                }
            })
            .sum::<i32>();
        (total / recent.len() as i32).clamp(-25, 25)
    }

    pub(super) fn resources_with_affordance(&self, kind: ItemAffordanceKind) -> Vec<String> {
        let mut resources = self
            .catalog
            .resources
            .iter()
            .filter(|resource| {
                resource
                    .affordances
                    .iter()
                    .any(|affordance| affordance.kind == kind)
            })
            .map(|resource| (resource.consumption_priority, resource.id.clone()))
            .collect::<Vec<_>>();
        resources.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        resources.into_iter().map(|(_, id)| id).collect()
    }

    pub(super) fn military_demand_requirements_for_stage(
        &self,
        stage: WarStage,
    ) -> (Vec<ResourceStack>, i32, u8) {
        let food = self
            .food_resource_ids_sorted()
            .into_iter()
            .find(|id| id == ResourceKind::Graos.id())
            .or_else(|| self.food_resource_ids_sorted().into_iter().next());
        let ready_food = self
            .food_resource_ids_sorted()
            .into_iter()
            .find(|id| id != ResourceKind::Graos.id())
            .or(food.clone());
        let tool = self
            .resources_with_affordance(ItemAffordanceKind::Tool)
            .into_iter()
            .next();
        let construction = self.resources_with_affordance(ItemAffordanceKind::ConstructionMaterial);
        let mut required = Vec::new();
        let mut push = |resource_id: Option<String>, amount: i32| {
            if let Some(resource_id) = resource_id {
                if amount > 0 {
                    required.push(ResourceStack {
                        resource_id,
                        amount,
                    });
                }
            }
        };
        match stage {
            WarStage::Mobilization => {
                push(food, 4);
                push(tool, 1);
                (required, 8, 80)
            }
            WarStage::Raids => {
                push(food, 5);
                push(tool, 1);
                push(Some(ResourceKind::MetalBruto.id().to_string()), 2);
                (required, 10, 86)
            }
            WarStage::Siege => {
                push(food, 8);
                push(tool, 1);
                for resource_id in construction.into_iter().take(2) {
                    push(Some(resource_id), 3);
                }
                (required, 14, 94)
            }
            WarStage::DecisiveBattle => {
                push(ready_food, 8);
                push(tool, 2);
                push(Some(ResourceKind::MetalBruto.id().to_string()), 3);
                (required, 20, 100)
            }
            WarStage::Occupation => {
                push(food, 6);
                push(construction.into_iter().next(), 3);
                (required, 10, 90)
            }
        }
    }

    pub(super) fn ensure_daily_military_demands_for_active_wars(&mut self) {
        let Some(local_polity_id) = self.polities.first().map(|polity| polity.id) else {
            return;
        };
        let active_wars = self
            .wars
            .iter()
            .filter(|war| {
                war.status == WarStatus::Active
                    && (war.attacker_polity_id == local_polity_id
                        || war.defender_polity_id == local_polity_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        for war in active_wars {
            if self.military_demands.iter().any(|demand| {
                demand.war_id == war.id
                    && demand.polity_id == local_polity_id
                    && demand.created_day == self.day
                    && demand.stage == war.stage
            }) {
                continue;
            }
            let (required, cash_required, priority) =
                self.military_demand_requirements_for_stage(war.stage);
            let demand_id = self.next_military_demand_id;
            self.next_military_demand_id += 1;
            let mut demand = MilitaryDemand {
                id: demand_id,
                war_id: war.id,
                polity_id: local_polity_id,
                stage: war.stage,
                required,
                delivered: Vec::new(),
                cash_required,
                cash_delivered: 0,
                target_territory_id: war.target_territory_ids.first().copied(),
                priority,
                deadline_day: self.day + 1,
                status: MilitaryDemandStatus::Open,
                shortage_score: 0,
                created_day: self.day,
            };
            Self::recalculate_military_demand_status(&mut demand);
            let missing = Self::missing_military_resources_for_demand(&demand)
                .into_iter()
                .map(|stack| {
                    format!(
                        "{} x{}",
                        self.resource_display_name(&stack.resource_id),
                        stack.amount
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            self.military_demands.push(demand);
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: 0,
                target: None,
                kind: EventKind::MilitarySupply,
                summary: format!(
                    "Demanda militar #{} aberta para guerra #{} em {:?}: recursos=[{}], caixa={} moedas.",
                    demand_id, war.id, war.stage, missing, cash_required
                ),
                impact_tags: vec![
                    "guerra".to_string(),
                    "suprimento_militar".to_string(),
                    format!("war:{}", war.id),
                ],
            });
        }
    }

    pub(super) fn settle_due_military_demands(&mut self) -> Result<()> {
        let due_ids = self
            .military_demands
            .iter()
            .filter(|demand| {
                demand.deadline_day <= self.day
                    && matches!(
                        demand.status,
                        MilitaryDemandStatus::Open | MilitaryDemandStatus::PartiallySupplied
                    )
            })
            .map(|demand| demand.id)
            .collect::<Vec<_>>();
        for demand_id in due_ids {
            let Some(index) = self
                .military_demands
                .iter()
                .position(|demand| demand.id == demand_id)
            else {
                continue;
            };
            Self::recalculate_military_demand_status(&mut self.military_demands[index]);
            let demand = self.military_demands[index].clone();
            let required_units: i32 = demand.required.iter().map(|stack| stack.amount).sum();
            let delivered_units: i32 = demand.delivered.iter().map(|stack| stack.amount).sum();
            let required_total = required_units + demand.cash_required;
            let delivered_total = delivered_units + demand.cash_delivered;
            let ratio = if required_total <= 0 {
                100
            } else {
                delivered_total * 100 / required_total
            };
            if let Some(demand_mut) = self
                .military_demands
                .iter_mut()
                .find(|demand| demand.id == demand_id)
            {
                demand_mut.status = if demand_mut.shortage_score <= 0 {
                    MilitaryDemandStatus::Satisfied
                } else {
                    MilitaryDemandStatus::Failed
                };
            }
            let readiness_delta = if ratio >= 80 {
                6
            } else if ratio >= 40 {
                1
            } else {
                -7
            };
            if let Some(polity) = self
                .polities
                .iter_mut()
                .find(|polity| polity.id == demand.polity_id)
            {
                polity.military_readiness =
                    (polity.military_readiness + readiness_delta).clamp(0, 100);
            }
            self.apply_military_demand_social_effects(&demand, ratio)?;
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: 0,
                target: None,
                kind: EventKind::MilitarySupply,
                summary: format!(
                    "Demanda militar #{} da guerra #{} fechou com {}% de atendimento; prontidao {:+}.",
                    demand.id, demand.war_id, ratio, readiness_delta
                ),
                impact_tags: vec![
                    "guerra".to_string(),
                    "suprimento_militar".to_string(),
                    format!("war:{}", demand.war_id),
                ],
            });
        }
        Ok(())
    }

    pub(super) fn apply_military_demand_social_effects(
        &mut self,
        demand: &MilitaryDemand,
        ratio: i32,
    ) -> Result<()> {
        let food_shortage: i32 = Self::missing_military_resources_for_demand(demand)
            .into_iter()
            .filter(|stack| self.is_food_resource(&stack.resource_id))
            .map(|stack| stack.amount)
            .sum();
        let cash_shortage = (demand.cash_required - demand.cash_delivered).max(0);
        let shortage = demand.shortage_score;
        if ratio < 80 {
            let affected_households: Vec<(BuildingId, u64)> = self
                .households
                .iter()
                .take(4)
                .map(|household| {
                    (
                        household.id,
                        household.member_ids.first().copied().unwrap_or(0),
                    )
                })
                .collect::<Vec<_>>();
            for (household_id, actor_id) in &affected_households {
                let agenda_tag = if food_shortage > 0 {
                    "motim_comida"
                } else if cash_shortage > 0 {
                    "boicote_imposto"
                } else {
                    "depor_lider"
                };
                let domain = if food_shortage > 0 {
                    PolicyDomain::Rationing
                } else if cash_shortage > 0 {
                    PolicyDomain::Tax
                } else {
                    PolicyDomain::Justice
                };
                let proposed_value = if food_shortage > 0 {
                    "priorizar_lares".to_string()
                } else if cash_shortage > 0 {
                    "recusar_custo_guerra".to_string()
                } else {
                    "trocar_lideranca".to_string()
                };
                self.political_pressures.push(PoliticalPressure {
                    actor_id: *actor_id,
                    household_id: Some(*household_id),
                    agenda_tag: agenda_tag.to_string(),
                    domain,
                    proposed_value,
                    intensity: (shortage / 2 + 8).clamp(5, 40),
                    reason: format!(
                        "demanda militar #{} falhou durante a guerra #{}",
                        demand.id, demand.war_id
                    ),
                    day: self.day,
                    tick: self.tick_of_day,
                });
            }
        }
        let agent_ids = self.agent_ids();
        for agent_id in agent_ids {
            let entity = self.find_agent_entity(agent_id)?;
            let role_id = self.agent_role_id(agent_id).unwrap_or_default();
            let mut entity_mut = self.world.entity_mut(entity);
            if let Some(mut psychology) = entity_mut.get_mut::<PsychologicalStateComponent>() {
                let mut delta = PsychologicalState::zero_delta();
                if ratio >= 80 {
                    delta.pride = 6;
                    delta.hope = 4;
                } else {
                    delta.fear = (shortage / 8).clamp(2, 10);
                    delta.anger = (shortage / 10).clamp(1, 8);
                    delta.humiliation = (cash_shortage / 8).clamp(0, 6);
                }
                psychology.0.add_delta(
                    &delta,
                    self.day,
                    format!(
                        "suprimento militar #{} da guerra #{}",
                        demand.id, demand.war_id
                    ),
                );
            }
            if let Some(mut perception) = entity_mut.get_mut::<InstitutionalPerceptionComponent>() {
                if ratio >= 80 {
                    perception.0.war_support += 3;
                    perception.0.leader_legitimacy += 2;
                    perception.0.perceived_fairness += 1;
                } else {
                    perception.0.war_support -= (shortage / 8).clamp(1, 8);
                    perception.0.leader_legitimacy -= (shortage / 10).clamp(1, 6);
                    perception.0.perceived_fairness -= (food_shortage / 3).clamp(0, 5);
                    perception.0.tax_legitimacy -= (cash_shortage / 6).clamp(0, 5);
                    perception.0.fear_of_authority += 1;
                }
                if role_id == Role::Guard.id() && ratio >= 80 {
                    perception.0.war_support += 2;
                }
                perception.0.notes.push(format!(
                    "suprimento militar #{} da guerra #{} atendido em {}%",
                    demand.id, demand.war_id, ratio
                ));
                perception.0.last_updated_day = self.day;
                perception.0.clamp_all();
            }
            if ratio < 50 {
                if let Some(mut state) = entity_mut.get_mut::<StateComponent>() {
                    state.0.stress = (state.0.stress + 3).clamp(0, 100);
                    if food_shortage > 0 {
                        state.0.hunger = (state.0.hunger + 2).clamp(0, 100);
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn war_stage_impact(stage: WarStage) -> (i32, i32, i32, i32, i32) {
        match stage {
            WarStage::Mobilization => (8, 4, 0, 0, 4),
            WarStage::Raids => (12, 8, 3, 5, 8),
            WarStage::Siege => (18, 14, 7, 12, 14),
            WarStage::DecisiveBattle => (25, 22, 4, 8, 20),
            WarStage::Occupation => (10, 16, 3, 4, 18),
        }
    }

    pub(super) fn apply_daily_war_impacts(&mut self, war: &WarState) -> Result<()> {
        let local_polity_id = self.polities.first().map(|polity| polity.id).unwrap_or(1);
        let local_involved =
            war.attacker_polity_id == local_polity_id || war.defender_polity_id == local_polity_id;
        if !local_involved {
            return Ok(());
        }

        let (treasury_cost, stress_inc, hunger_inc, stock_loss_percent, faction_rage_bonus) =
            Self::war_stage_impact(war.stage);
        self.village_economy.public_treasury =
            (self.village_economy.public_treasury - treasury_cost).max(0);
        if let Some(polity) = self
            .polities
            .iter_mut()
            .find(|polity| polity.id == local_polity_id)
        {
            polity.treasury = (polity.treasury - treasury_cost).max(0);
            polity.military_readiness = (polity.military_readiness + 2).clamp(0, 100);
        }

        let affected_agents = self.agent_ids();
        let mut battle_casualties = Vec::new();
        for agent_id in &affected_agents {
            let role_id = self.agent_role_id(*agent_id).unwrap_or_default();
            let entity = self.find_agent_entity(*agent_id)?;
            let mut entity_mut = self.world.entity_mut(entity);
            if let Some(status) = entity_mut.get::<LifeStatusComponent>()
                && status.0 != AgentLifeStatus::Vivo
            {
                continue;
            }
            if let Some(mut psychology) = entity_mut.get_mut::<PsychologicalStateComponent>() {
                let mut delta = PsychologicalState::zero_delta();
                delta.fear = (stress_inc / 4).clamp(1, 12);
                delta.trauma =
                    if matches!(war.stage, WarStage::DecisiveBattle | WarStage::Occupation) {
                        8
                    } else {
                        (stress_inc / 6).clamp(0, 5)
                    };
                if role_id == Role::Guard.id() || role_id == Role::Headman.id() {
                    delta.pride = 3;
                }
                if hunger_inc > 0 {
                    delta.anger += hunger_inc.clamp(1, 8);
                }
                psychology.0.add_delta(
                    &delta,
                    self.day,
                    format!(
                        "impacto psicologico da guerra #{} em {:?}",
                        war.id, war.stage
                    ),
                );
            }
            if let Some(mut state) = entity_mut.get_mut::<StateComponent>() {
                let duty_stress = if role_id == Role::Guard.id() || role_id == Role::Headman.id() {
                    5
                } else {
                    0
                };
                state.0.stress = (state.0.stress + stress_inc + duty_stress).clamp(0, 100);
                state.0.hunger = (state.0.hunger + hunger_inc).clamp(0, 100);
                state.0.mood = (state.0.mood - stress_inc / 3).clamp(0, 100);
            }
            if war.stage == WarStage::DecisiveBattle
                && (role_id == Role::Guard.id()
                    || (battle_casualties.len() < 2 && role_id == Role::Farmer.id()))
            {
                if let Some(mut injury) = entity_mut.get_mut::<InjuryComponent>() {
                    injury.0.light_wounds = injury.0.light_wounds.saturating_add(1);
                    injury.0.pain = (injury.0.pain + 12).clamp(0, 100);
                    injury.0.bleeding = (injury.0.bleeding + 2).clamp(0, 100);
                }
                if let Some(mut state) = entity_mut.get_mut::<StateComponent>() {
                    state.0.health = (state.0.health - 12).clamp(0, 100);
                    if state.0.health <= 0
                        && let Some(mut status) = entity_mut.get_mut::<LifeStatusComponent>()
                    {
                        status.0 = AgentLifeStatus::Morto;
                    }
                }
                battle_casualties.push(*agent_id);
            }
            if let Some(mut perception) = entity_mut.get_mut::<InstitutionalPerceptionComponent>() {
                let duty_bonus = if role_id == Role::Guard.id() || role_id == Role::Headman.id() {
                    3
                } else {
                    0
                };
                perception.0.leader_legitimacy -= stress_inc / 6;
                perception.0.war_support += duty_bonus - stress_inc / 7 - hunger_inc / 3;
                perception.0.tax_legitimacy -= treasury_cost / 8;
                perception.0.fear_of_authority += stress_inc / 8;
                if matches!(war.stage, WarStage::Siege | WarStage::DecisiveBattle) {
                    perception.0.perceived_fairness -= 2;
                }
                if war.stage == WarStage::Occupation {
                    perception.0.leader_legitimacy -= 6;
                    perception.0.war_support -= 6;
                }
                perception.0.notes.push(format!(
                    "impacto institucional da guerra #{} em {:?}",
                    war.id, war.stage
                ));
                perception.0.last_updated_day = self.day;
                perception.0.clamp_all();
            }
        }

        for agent_id in affected_agents.iter().take(6) {
            self.add_memory(
                *agent_id,
                MemoryKind::Reflection,
                format!(
                    "A guerra #{} em {:?} pesa sobre a vila: medo, custo e escassez se acumulam.",
                    war.id, war.stage
                ),
                vec!["guerra".to_string(), format!("war:{}", war.id)],
                8,
                Vec::new(),
            )?;
        }

        for territory_id in &war.target_territory_ids {
            let Some(building_ids) = self
                .territories
                .iter()
                .find(|territory| territory.id == *territory_id)
                .map(|territory| territory.building_ids.clone())
            else {
                continue;
            };
            for establishment in &mut self.establishments {
                if !establishment
                    .building_id
                    .is_some_and(|building_id| building_ids.contains(&building_id))
                {
                    continue;
                }
                for resource_id in ["graos", "ferramentas", "metal_bruto", "madeira"] {
                    if let Some(stack) = establishment
                        .stock
                        .iter_mut()
                        .find(|stack| stack.resource_id == resource_id)
                    {
                        let loss = (stack.amount * stock_loss_percent / 100).max(0);
                        stack.amount -= loss;
                    }
                }
            }
        }

        for faction in &mut self.political_factions {
            let aligned = matches!(
                faction.objective,
                Some(FactionObjective::FoodRiot { .. })
                    | Some(FactionObjective::TaxBoycott { .. })
                    | Some(FactionObjective::DeposeLeader { .. })
            );
            if aligned {
                faction.rage = (faction.rage + faction_rage_bonus).clamp(0, 100);
            }
        }

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: 0,
            target: None,
            kind: EventKind::InstitutionalDispute,
            summary: format!(
                "Impacto da guerra #{} em {:?}: custo={} stress={} fome={} perda_estoque={}%.",
                war.id, war.stage, treasury_cost, stress_inc, hunger_inc, stock_loss_percent
            ),
            impact_tags: vec![
                "guerra".to_string(),
                "impacto_guerra".to_string(),
                format!("war:{}", war.id),
            ],
        });

        if !battle_casualties.is_empty() {
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: 0,
                target: battle_casualties.first().copied(),
                kind: EventKind::Violence,
                summary: format!(
                    "A batalha decisiva da guerra #{} deixou feridos: {:?}.",
                    war.id, battle_casualties
                ),
                impact_tags: vec![
                    "guerra".to_string(),
                    "baixas".to_string(),
                    format!("war:{}", war.id),
                ],
            });
        }
        Ok(())
    }
}

impl Simulation {
    pub(super) fn agent_chaos_pressure(&mut self, agent_id: u64) -> Result<u32> {
        let entity = self.find_agent_entity(agent_id)?;
        let (state, profile, relations, injury) = {
            let entry = self.world.entity(entity);
            let state = entry
                .get::<StateComponent>()
                .map(|s| s.0.clone())
                .ok_or_else(|| anyhow!("missing state"))?;
            let profile = entry
                .get::<ProfileComponent>()
                .map(|p| p.0.clone())
                .ok_or_else(|| anyhow!("missing profile"))?;
            let relations = entry
                .get::<RelationComponent>()
                .map(|r| r.0.clone())
                .unwrap_or_default();
            let injury = entry
                .get::<InjuryComponent>()
                .map(|i| i.0.clone())
                .ok_or_else(|| anyhow!("missing injury"))?;
            (state, profile, relations, injury)
        };
        let hh_treasury = self
            .household_id_for_agent(agent_id)
            .and_then(|h_id| self.household_by_id(h_id))
            .map(|h| h.treasury)
            .unwrap_or(0);
        let food_crisis = self
            .household_id_for_agent(agent_id)
            .and_then(|h_id| self.household_by_id(h_id))
            .map(|h| h.food_crisis_level)
            .unwrap_or(0);
        let war_pressure = self.active_local_war_pressure();
        let institutional_pressure = self
            .institutional_perception(agent_id)
            .map(|perception| {
                let legitimacy_loss = [
                    perception.leader_legitimacy,
                    perception.justice_legitimacy,
                    perception.tax_legitimacy,
                    perception.rationing_legitimacy,
                    perception.guard_trust,
                    perception.war_support,
                ]
                .iter()
                .filter(|value| **value < 0)
                .map(|value| value.abs())
                .sum::<i32>()
                    / 12;
                let corruption = perception.perceived_corruption.max(0) / 8;
                let fear =
                    if perception.fear_of_authority >= 45 && perception.leader_legitimacy <= -20 {
                        8
                    } else {
                        perception.fear_of_authority.max(0) / 18
                    };
                (legitimacy_loss + corruption + fear).clamp(0, 30) as u32
            })
            .unwrap_or(0);
        Ok((Self::compute_chaos_pressure(
            &state,
            &profile,
            &relations,
            &injury,
            hh_treasury,
            food_crisis,
        ) + war_pressure
            + institutional_pressure)
            .min(100))
    }

    pub(super) fn active_local_war_pressure(&self) -> u32 {
        let local_polity_id = self.polities.first().map(|polity| polity.id).unwrap_or(1);
        self.wars
            .iter()
            .filter(|war| {
                war.status == WarStatus::Active
                    && (war.attacker_polity_id == local_polity_id
                        || war.defender_polity_id == local_polity_id)
            })
            .map(|war| match war.stage {
                WarStage::Mobilization => 4,
                WarStage::Raids => 8,
                WarStage::Siege => 14,
                WarStage::DecisiveBattle => 20,
                WarStage::Occupation => 22,
            })
            .max()
            .unwrap_or(0)
    }

    pub(super) fn agent_role(&mut self, agent_id: u64) -> Result<String> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<AgentCore>()
            .ok_or_else(|| anyhow!("missing agent core"))?
            .role_id
            .clone())
    }

    pub(super) fn village_name_by_index(&self, index: usize) -> &str {
        match index {
            0 => &self.village_name,
            1 => "Vale Verde",
            2 => "Pedra Ruiva",
            _ => "Santa Bruma",
        }
    }

    pub(super) fn check_faction_founding(&mut self) -> Result<()> {
        let agent_ids = self.agent_ids();
        for agent_id in agent_ids {
            let in_faction = self
                .political_factions
                .iter()
                .any(|f| f.member_ids.contains(&agent_id));
            if in_faction {
                continue;
            }

            let coord = self.debug_agent_position(agent_id)?;
            let v_idx = self.village_index_of_coord(coord);
            let v_name = self.village_name_by_index(v_idx).to_string();
            let founder_name = self.agent_name(agent_id)?;

            // 1. Motim de Comida (FoodRiot)
            let hunger = self.agent_state(agent_id)?.hunger;
            if hunger >= 75 {
                let farm_building = self
                    .spatial
                    .buildings
                    .iter()
                    .filter(|b| b.kind == LocationKind::Farm)
                    .min_by_key(|b| b.entrance.manhattan(coord))
                    .cloned();
                if let Some(farm) = farm_building {
                    let faction_id = self.next_political_faction_id;
                    self.next_political_faction_id += 1;
                    let name = format!("Revoltados do Celeiro de {}", v_name);
                    let influence = self.political_influence(agent_id);
                    self.political_factions.push(PoliticalFaction {
                        id: faction_id,
                        name: name.clone(),
                        agenda_tag: "motim_comida".to_string(),
                        domain: PolicyDomain::Rationing,
                        proposed_value: "produtores".to_string(),
                        founder_id: agent_id,
                        member_ids: vec![agent_id],
                        influence,
                        support_issue_ids: Vec::new(),
                        opposition_issue_ids: Vec::new(),
                        objective: Some(FactionObjective::FoodRiot {
                            barn_building_id: farm.id,
                            target_grains: 15,
                            grains_stolen: 0,
                        }),
                        is_action_active: false,
                        rage: 10,
                    });
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: agent_id,
                        target: None,
                        kind: EventKind::FactionShift,
                        summary: format!(
                            "{} funda a facÃ§Ã£o '{}' exigindo grÃ£os do celeiro.",
                            founder_name, name
                        ),
                        impact_tags: vec![
                            "politica".to_string(),
                            "faccao".to_string(),
                            "motim_comida".to_string(),
                        ],
                    });
                    continue;
                }
            }

            // 2. Boicote de Impostos (TaxBoycott)
            if let Some(household_id) = self.household_id_for_agent(agent_id) {
                if let Some(household) = self.household_by_id(household_id) {
                    if household.tax_arrears >= 10 {
                        let faction_id = self.next_political_faction_id;
                        self.next_political_faction_id += 1;
                        let name = format!("Liga Anti-Imposto de {}", v_name);
                        let influence = self.political_influence(agent_id);
                        self.political_factions.push(PoliticalFaction {
                            id: faction_id,
                            name: name.clone(),
                            agenda_tag: "boicote_imposto".to_string(),
                            domain: PolicyDomain::Tax,
                            proposed_value: "reduzir".to_string(),
                            founder_id: agent_id,
                            member_ids: vec![agent_id],
                            influence,
                            support_issue_ids: Vec::new(),
                            opposition_issue_ids: Vec::new(),
                            objective: Some(FactionObjective::TaxBoycott {
                                day_activated: self.day,
                            }),
                            is_action_active: false,
                            rage: 10,
                        });
                        self.push_event(WorldEvent {
                            day: self.day,
                            tick: self.tick_of_day,
                            actor: agent_id,
                            target: None,
                            kind: EventKind::FactionShift,
                            summary: format!(
                                "{} funda a facÃ§Ã£o '{}' boicotando o imposto diÃ¡rio.",
                                founder_name, name
                            ),
                            impact_tags: vec![
                                "politica".to_string(),
                                "faccao".to_string(),
                                "boicote_imposto".to_string(),
                            ],
                        });
                        continue;
                    }
                }
            }

            // 3. Derrubar o LÃ­der (DeposeLeader)
            let chaos = self.agent_chaos_pressure(agent_id)?;
            let profile = self.agent_profile(agent_id)?;
            let is_rebel = profile.traits.contains(&"rebelde".to_string())
                || profile.traits.contains(&"vingativo".to_string())
                || profile.traits.contains(&"oportunista".to_string());
            if chaos >= 70 && is_rebel {
                let mut leader_id_opt = None;
                for (a_id, role_id) in self.agent_role_pairs() {
                    if role_id == Role::Headman.id() {
                        let a_pos = self.debug_agent_position(a_id)?;
                        if self.village_index_of_coord(a_pos) == v_idx {
                            leader_id_opt = Some(a_id);
                            break;
                        }
                    }
                }
                if let Some(leader_agent_id) = leader_id_opt {
                    let faction_id = self.next_political_faction_id;
                    self.next_political_faction_id += 1;
                    let name = format!("Rebeldes Conspiradores de {}", v_name);
                    let influence = self.political_influence(agent_id);
                    self.political_factions.push(PoliticalFaction {
                        id: faction_id,
                        name: name.clone(),
                        agenda_tag: "depor_lider".to_string(),
                        domain: PolicyDomain::Justice,
                        proposed_value: "normal".to_string(),
                        founder_id: agent_id,
                        member_ids: vec![agent_id],
                        influence,
                        support_issue_ids: Vec::new(),
                        opposition_issue_ids: Vec::new(),
                        objective: Some(FactionObjective::DeposeLeader { leader_agent_id }),
                        is_action_active: false,
                        rage: 15,
                    });
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: agent_id,
                        target: Some(leader_agent_id),
                        kind: EventKind::FactionShift,
                        summary: format!(
                            "{} funda a facÃ§Ã£o '{}' conspirando para depor o LÃ­der.",
                            founder_name, name
                        ),
                        impact_tags: vec![
                            "politica".to_string(),
                            "faccao".to_string(),
                            "depor_lider".to_string(),
                        ],
                    });
                    continue;
                }
            }

            // 4. JustiÃ§a Vigilante (VigilanteJustice)
            let mut vigilante_case_opt = None;
            for case in &self.crime_cases {
                if case.victim_id == Some(agent_id)
                    && matches!(
                        case.status,
                        CrimeCaseStatus::Open | CrimeCaseStatus::Investigating
                    )
                    && self.day >= case.opened_day + 1
                {
                    if let Some(suspect_id) = case.suspect_id {
                        vigilante_case_opt = Some((suspect_id, case.id));
                        break;
                    }
                }
            }
            if let Some((suspect_agent_id, crime_case_id)) = vigilante_case_opt {
                let faction_id = self.next_political_faction_id;
                self.next_political_faction_id += 1;
                let name = format!("Vigilantes de {}", v_name);
                let influence = self.political_influence(agent_id);
                self.political_factions.push(PoliticalFaction {
                    id: faction_id,
                    name: name.clone(),
                    agenda_tag: "justica_vigilante".to_string(),
                    domain: PolicyDomain::Justice,
                    proposed_value: "severa".to_string(),
                    founder_id: agent_id,
                    member_ids: vec![agent_id],
                    influence,
                    support_issue_ids: Vec::new(),
                    opposition_issue_ids: Vec::new(),
                    objective: Some(FactionObjective::VigilanteJustice {
                        suspect_agent_id,
                        crime_case_id,
                    }),
                    is_action_active: false,
                    rage: 20,
                });
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: agent_id,
                    target: Some(suspect_agent_id),
                    kind: EventKind::FactionShift,
                    summary: format!(
                        "{} funda a facÃ§Ã£o '{}' para caÃ§ar e punir o suspeito.",
                        founder_name, name
                    ),
                    impact_tags: vec![
                        "politica".to_string(),
                        "faccao".to_string(),
                        "justica_vigilante".to_string(),
                    ],
                });
                continue;
            }

            // 5. Defensores do ErÃ¡rio (Aumentar Imposto)
            if self.village_economy.public_treasury < 20 {
                if let Ok(role_id) = self.agent_role(agent_id) {
                    if role_id == Role::Guard.id() || role_id == Role::Headman.id() {
                        let faction_id = self.next_political_faction_id;
                        self.next_political_faction_id += 1;
                        let name = format!("Defensores do ErÃ¡rio de {}", v_name);
                        let influence = self.political_influence(agent_id);
                        self.political_factions.push(PoliticalFaction {
                            id: faction_id,
                            name: name.clone(),
                            agenda_tag: "aumentar_imposto".to_string(),
                            domain: PolicyDomain::Tax,
                            proposed_value: "aumentar".to_string(),
                            founder_id: agent_id,
                            member_ids: vec![agent_id],
                            influence,
                            support_issue_ids: Vec::new(),
                            opposition_issue_ids: Vec::new(),
                            objective: None,
                            is_action_active: false,
                            rage: 0,
                        });
                        self.push_event(WorldEvent {
                            day: self.day,
                            tick: self.tick_of_day,
                            actor: agent_id,
                            target: None,
                            kind: EventKind::FactionShift,
                            summary: format!(
                                "{} funda a facÃ§Ã£o '{}' para restaurar o tesouro pÃºblico.",
                                founder_name, name
                            ),
                            impact_tags: vec![
                                "politica".to_string(),
                                "faccao".to_string(),
                                "aumentar_imposto".to_string(),
                            ],
                        });
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn update_faction_rage_and_activity(&mut self) -> Result<()> {
        let mut factions = self.political_factions.clone();
        for faction in &mut factions {
            if faction.is_action_active {
                continue;
            }

            let mut delta_rage = 0;
            for &member_id in &faction.member_ids {
                if let Ok(state) = self.agent_state(member_id) {
                    match faction.objective {
                        Some(FactionObjective::FoodRiot { .. }) => {
                            if state.hunger >= 50 {
                                delta_rage += 2;
                            }
                        }
                        Some(FactionObjective::TaxBoycott { .. }) => {
                            if let Some(household_id) = self.household_id_for_agent(member_id) {
                                if let Some(household) = self.household_by_id(household_id) {
                                    if household.tax_arrears > 0 {
                                        delta_rage += 2;
                                    }
                                }
                            }
                        }
                        Some(FactionObjective::DeposeLeader { .. }) => {
                            let chaos = self.agent_chaos_pressure(member_id)?;
                            if chaos >= 50 {
                                delta_rage += 3;
                            }
                        }
                        Some(FactionObjective::VigilanteJustice {
                            suspect_agent_id, ..
                        }) => {
                            let resentment = self
                                .relation_between(member_id, suspect_agent_id)
                                .resentment;
                            if resentment >= 20 {
                                delta_rage += 3;
                            }
                        }
                        None => {}
                    }
                }
                if let Some(perception) = self.institutional_perception(member_id) {
                    let target_legitimacy = match faction.objective {
                        Some(FactionObjective::FoodRiot { .. }) => perception.rationing_legitimacy,
                        Some(FactionObjective::TaxBoycott { .. }) => perception.tax_legitimacy,
                        Some(FactionObjective::DeposeLeader { .. }) => perception.leader_legitimacy,
                        Some(FactionObjective::VigilanteJustice { .. }) => {
                            perception.justice_legitimacy
                        }
                        None => 0,
                    };
                    if target_legitimacy <= -30 {
                        delta_rage += 2;
                    } else if target_legitimacy >= 35 {
                        delta_rage -= 1;
                    }
                    if perception.fear_of_authority >= 55 && target_legitimacy <= -25 {
                        delta_rage -= 1;
                    }
                }
            }

            faction.rage = (faction.rage + delta_rage).clamp(0, 100);
            faction.influence = faction
                .member_ids
                .iter()
                .map(|&id| self.political_influence(id))
                .sum::<i32>();

            let min_members = if self.agent_ids().len() < 8 { 1 } else { 3 };
            if faction.member_ids.len() >= min_members && faction.rage >= 50 {
                faction.is_action_active = true;
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: faction.founder_id,
                    target: None,
                    kind: EventKind::InstitutionalDispute,
                    summary: format!(
                        "A facÃ§Ã£o '{}' ativa aÃ§Ã£o fÃ­sica no mundo! Objetivo: {:?}",
                        faction.name, faction.objective
                    ),
                    impact_tags: vec![
                        "politica".to_string(),
                        "faccao".to_string(),
                        faction.agenda_tag.clone(),
                        "motim".to_string(),
                    ],
                });
            }
        }
        self.political_factions = factions;
        Ok(())
    }

    pub(super) fn update_insurrections(&mut self) -> Result<()> {
        let local_polity_id = self.polities.first().map(|polity| polity.id).unwrap_or(1);
        let Some(target_territory_id) = self.territories.first().map(|territory| territory.id)
        else {
            return Ok(());
        };

        let active_rebel_factions = self
            .political_factions
            .iter()
            .filter(|faction| {
                faction.is_action_active
                    && faction.rage >= 50
                    && matches!(
                        faction.objective,
                        Some(FactionObjective::FoodRiot { .. })
                            | Some(FactionObjective::TaxBoycott { .. })
                            | Some(FactionObjective::DeposeLeader { .. })
                    )
            })
            .cloned()
            .collect::<Vec<_>>();

        if active_rebel_factions.is_empty() {
            return Ok(());
        }

        let faction_ids = active_rebel_factions
            .iter()
            .map(|faction| faction.id)
            .collect::<Vec<_>>();
        let support = active_rebel_factions
            .iter()
            .map(|faction| faction.influence + faction.rage)
            .sum::<i32>();
        let repression = self
            .agent_role_pairs()
            .into_iter()
            .filter(|(_, role_id)| role_id == Role::Guard.id() || role_id == Role::Headman.id())
            .map(|(agent_id, _)| self.political_influence(agent_id))
            .sum::<i32>();

        let existing_index = self.insurrections.iter().position(|insurrection| {
            insurrection.status == InsurrectionStatus::Active
                && insurrection.target_polity_id == local_polity_id
                && insurrection.target_territory_id == target_territory_id
        });

        let insurrection_id = if let Some(index) = existing_index {
            let insurrection = &mut self.insurrections[index];
            for faction_id in &faction_ids {
                if !insurrection.faction_ids.contains(faction_id) {
                    insurrection.faction_ids.push(*faction_id);
                }
            }
            insurrection.popular_support = support;
            insurrection.repression = repression;
            insurrection.id
        } else {
            let id = self.next_insurrection_id;
            self.next_insurrection_id += 1;
            self.insurrections.push(InsurrectionState {
                id,
                faction_ids: faction_ids.clone(),
                target_polity_id: local_polity_id,
                rebel_polity_id: None,
                target_territory_id,
                popular_support: support,
                repression,
                stage: InsurrectionStage::Agitation,
                status: InsurrectionStatus::Active,
                linked_war_id: None,
                started_day: self.day,
                ended_day: None,
                summary: "Faccoes locais iniciam agitacao contra o controlador.".to_string(),
            });
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: active_rebel_factions
                    .first()
                    .map(|faction| faction.founder_id)
                    .unwrap_or(0),
                target: None,
                kind: EventKind::InstitutionalDispute,
                summary: format!(
                    "Insurreicao #{} comeca a se formar com faccoes {:?}.",
                    id, faction_ids
                ),
                impact_tags: vec!["insurreicao".to_string(), "faccao".to_string()],
            });
            id
        };

        let Some(index) = self
            .insurrections
            .iter()
            .position(|insurrection| insurrection.id == insurrection_id)
        else {
            return Ok(());
        };

        let mut create_civil_war = false;
        let mut suppress = false;
        {
            let insurrection = &mut self.insurrections[index];
            if support < repression - 40 {
                suppress = true;
            } else if support >= 220 {
                insurrection.stage = InsurrectionStage::CivilWar;
                create_civil_war = insurrection.linked_war_id.is_none();
            } else if support >= 160 {
                insurrection.stage = InsurrectionStage::OrganizedRevolt;
            } else if support >= 100 {
                insurrection.stage = InsurrectionStage::Riot;
            } else {
                insurrection.stage = InsurrectionStage::Agitation;
            }
        }

        if suppress {
            let suppressed_id = {
                let insurrection = &mut self.insurrections[index];
                insurrection.stage = InsurrectionStage::Suppressed;
                insurrection.status = InsurrectionStatus::Suppressed;
                insurrection.ended_day = Some(self.day);
                insurrection.id
            };
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: 0,
                target: None,
                kind: EventKind::InstitutionalDispute,
                summary: format!(
                    "Insurreicao #{} foi reprimida: apoio={} repressao={}.",
                    suppressed_id, support, repression
                ),
                impact_tags: vec!["insurreicao".to_string(), "repressao".to_string()],
            });
            return Ok(());
        }

        if create_civil_war {
            let rebel_polity_id = self.next_polity_id;
            self.next_polity_id += 1;
            self.polities.push(Polity {
                id: rebel_polity_id,
                name: format!("Comuna Rebelde #{}", insurrection_id),
                ruler_agent_id: active_rebel_factions
                    .first()
                    .map(|faction| faction.founder_id),
                capital_territory_id: Some(target_territory_id),
                treasury: 0,
                military_readiness: (support / 4).clamp(10, 80),
            });
            let war_id = self.next_war_id;
            self.next_war_id += 1;
            self.wars.push(WarState {
                id: war_id,
                attacker_polity_id: rebel_polity_id,
                defender_polity_id: local_polity_id,
                target_territory_ids: vec![target_territory_id],
                attacker_score: 0,
                defender_score: 0,
                stage: WarStage::Mobilization,
                status: WarStatus::Active,
                winner_polity_id: None,
                started_day: self.day,
                ended_day: None,
                summary: format!("Guerra civil criada pela insurreicao #{}.", insurrection_id),
            });
            if let Some(insurrection) = self.insurrections.get_mut(index) {
                insurrection.rebel_polity_id = Some(rebel_polity_id);
                insurrection.linked_war_id = Some(war_id);
                insurrection.summary = format!(
                    "Insurreicao escalou para guerra civil #{} contra polity {}.",
                    war_id, local_polity_id
                );
            }
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: active_rebel_factions
                    .first()
                    .map(|faction| faction.founder_id)
                    .unwrap_or(0),
                target: None,
                kind: EventKind::InstitutionalDispute,
                summary: format!(
                    "Insurreicao #{} escalou para guerra civil #{}.",
                    insurrection_id, war_id
                ),
                impact_tags: vec![
                    "insurreicao".to_string(),
                    "guerra_civil".to_string(),
                    "guerra".to_string(),
                ],
            });
        }

        let linked_war = self.insurrections[index].linked_war_id;
        if let Some(war_id) = linked_war
            && let Some(war) = self.wars.iter().find(|war| war.id == war_id)
            && war.status == WarStatus::Won
        {
            let winner = war.winner_polity_id;
            let rebel_polity = self.insurrections[index].rebel_polity_id;
            let insurrection = &mut self.insurrections[index];
            if winner == rebel_polity {
                insurrection.stage = InsurrectionStage::Victorious;
                insurrection.status = InsurrectionStatus::Victorious;
            } else {
                insurrection.stage = InsurrectionStage::Suppressed;
                insurrection.status = InsurrectionStatus::Suppressed;
            }
            insurrection.ended_day = Some(self.day);
        }

        Ok(())
    }

    pub(super) fn check_faction_resolution(&mut self) -> Result<()> {
        let mut factions = self.political_factions.clone();
        let mut factions_to_remove = Vec::new();

        for faction in &mut factions {
            if !faction.is_action_active {
                continue;
            }

            let mut resolved = false;
            let mut success = false;
            let mut reason = String::new();

            let mut active_members = 0;
            for &member_id in &faction.member_ids {
                if self.can_agent_act(member_id)? {
                    active_members += 1;
                }
            }

            if active_members == 0 {
                resolved = true;
                success = false;
                reason = "todos os membros foram nocauteados ou detidos pelos guardas".to_string();
            } else if let Some(obj) = faction.objective {
                match obj {
                    FactionObjective::FoodRiot {
                        barn_building_id,
                        target_grains,
                        grains_stolen,
                    } => {
                        if grains_stolen >= target_grains {
                            resolved = true;
                            success = true;
                            reason = format!(
                                "saquearam com sucesso {} grÃ£os do Celeiro",
                                grains_stolen
                            );
                        } else {
                            if let Some(est) = self.establishment_by_building(barn_building_id) {
                                let available = Self::total_resource_amount(
                                    &est.stock,
                                    &ResourceKind::Graos.id().to_string(),
                                );
                                if available == 0 {
                                    resolved = true;
                                    success = false;
                                    reason = "o estoque de grÃ£os do Celeiro acabou completamente"
                                        .to_string();
                                }
                            }
                        }
                    }
                    FactionObjective::TaxBoycott { day_activated } => {
                        if self.day > day_activated {
                            resolved = true;
                            success = true;
                            reason = "resistiram com sucesso Ã  cobranÃ§a de impostos do dia"
                                .to_string();
                        }
                    }
                    FactionObjective::DeposeLeader { leader_agent_id } => {
                        let leader_state = self.agent_state(leader_agent_id)?;
                        if leader_state.health < 30 || leader_state.energy < 15 {
                            resolved = true;
                            success = true;
                            reason = "derrubaram com sucesso o LÃ­der local".to_string();
                            self.village_economy.daily_household_tax = 1;
                            self.local_norms.rationing_policy = RationingPolicy::Balanced;
                            if let Ok(leader_entity) = self.find_agent_entity(leader_agent_id) {
                                let mut leader_entity_mut = self.world.entity_mut(leader_entity);
                                let mut core = leader_entity_mut.get_mut::<AgentCore>().unwrap();
                                core.role_id = "normal".to_string();
                            }
                        }
                    }
                    FactionObjective::VigilanteJustice {
                        suspect_agent_id,
                        crime_case_id,
                    } => {
                        let suspect_state = self.agent_state(suspect_agent_id)?;
                        if suspect_state.health < 30 || suspect_state.energy < 15 {
                            resolved = true;
                            success = true;
                            reason = "fizeram justiÃ§a com as prÃ³prias mÃ£os punindo o suspeito"
                                .to_string();
                            if let Some(case) =
                                self.crime_cases.iter_mut().find(|c| c.id == crime_case_id)
                            {
                                case.status = CrimeCaseStatus::Punished;
                            }
                        }
                    }
                }
            }

            if resolved {
                faction.is_action_active = false;
                faction.rage = 0;
                for &member_id in &faction.member_ids {
                    self.clear_intent_navigation(member_id)?;
                }
                let outcome_str = if success { "Sucesso" } else { "Fracasso" };
                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: faction.founder_id,
                    target: None,
                    kind: EventKind::InstitutionalDispute,
                    summary: format!(
                        "AÃ§Ã£o da facÃ§Ã£o '{}' encerrada ({}): {}.",
                        faction.name, outcome_str, reason
                    ),
                    impact_tags: vec![
                        "politica".to_string(),
                        "faccao".to_string(),
                        faction.agenda_tag.clone(),
                        "resolvido".to_string(),
                    ],
                });
                factions_to_remove.push(faction.id);
            }
        }

        factions.retain(|f| !factions_to_remove.contains(&f.id));
        self.political_factions = factions;
        Ok(())
    }

    pub(super) fn apply_faction_action_overrides(&mut self) -> Result<()> {
        let agent_ids = self.agent_ids();
        for agent_id in agent_ids {
            let active_faction_opt = self
                .political_factions
                .iter()
                .find(|f| f.is_action_active && f.member_ids.contains(&agent_id))
                .cloned();

            if let Some(faction) = active_faction_opt {
                if let Some(obj) = faction.objective {
                    let current_pos = self.debug_agent_position(agent_id)?;
                    let mut target_coord = None;
                    let mut target_agent_id = None;

                    match obj {
                        FactionObjective::FoodRiot {
                            barn_building_id, ..
                        } => {
                            if let Some(building) = self.building_by_id(barn_building_id) {
                                target_coord = Some(building.entrance);
                            }
                        }
                        FactionObjective::TaxBoycott { .. } => {
                            let v_idx = self.village_index_of_coord(current_pos);
                            let guard_post = self
                                .spatial
                                .buildings
                                .iter()
                                .find(|b| {
                                    b.kind == LocationKind::GuardPost
                                        && self.village_index_of_coord(b.entrance) == v_idx
                                })
                                .cloned();
                            if let Some(gp) = guard_post {
                                target_coord = Some(gp.entrance);
                            }
                        }
                        FactionObjective::DeposeLeader { leader_agent_id } => {
                            target_agent_id = Some(leader_agent_id);
                            if let Ok(l_pos) = self.debug_agent_position(leader_agent_id) {
                                target_coord = Some(l_pos);
                            }
                        }
                        FactionObjective::VigilanteJustice {
                            suspect_agent_id, ..
                        } => {
                            target_agent_id = Some(suspect_agent_id);
                            if let Ok(s_pos) = self.debug_agent_position(suspect_agent_id) {
                                target_coord = Some(s_pos);
                            }
                        }
                    }

                    if let Some(dest) = target_coord {
                        let entity = self.find_agent_entity(agent_id)?;
                        let is_at_target = current_pos == dest || current_pos.manhattan(dest) <= 1;

                        if is_at_target {
                            let mut intent_kind = IntentKind::Trabalhar;

                            match obj {
                                FactionObjective::FoodRiot { .. } => {
                                    let mut guard_to_fight = None;
                                    for (other_id, role_id) in self.agent_role_pairs() {
                                        if role_id == Role::Guard.id()
                                            && self.can_agent_act(other_id)?
                                        {
                                            let other_pos = self.debug_agent_position(other_id)?;
                                            if current_pos.manhattan(other_pos) <= 1 {
                                                guard_to_fight = Some(other_id);
                                                break;
                                            }
                                        }
                                    }
                                    if let Some(guard_id) = guard_to_fight {
                                        intent_kind = IntentKind::Agredir;
                                        target_agent_id = Some(guard_id);
                                    } else {
                                        intent_kind = IntentKind::Trabalhar;
                                        target_agent_id = None;
                                    }
                                }
                                FactionObjective::TaxBoycott { .. } => {
                                    let mut guard_to_fight = None;
                                    for (other_id, role_id) in self.agent_role_pairs() {
                                        if role_id == Role::Guard.id()
                                            && self.can_agent_act(other_id)?
                                        {
                                            let other_pos = self.debug_agent_position(other_id)?;
                                            if current_pos.manhattan(other_pos) <= 1 {
                                                guard_to_fight = Some(other_id);
                                                break;
                                            }
                                        }
                                    }
                                    if let Some(guard_id) = guard_to_fight {
                                        intent_kind = IntentKind::Agredir;
                                        target_agent_id = Some(guard_id);
                                    } else {
                                        intent_kind = IntentKind::Refletir;
                                        target_agent_id = None;
                                    }
                                }
                                FactionObjective::DeposeLeader { leader_agent_id } => {
                                    intent_kind = IntentKind::Agredir;
                                    target_agent_id = Some(leader_agent_id);
                                }
                                FactionObjective::VigilanteJustice {
                                    suspect_agent_id, ..
                                } => {
                                    intent_kind = IntentKind::Agredir;
                                    target_agent_id = Some(suspect_agent_id);
                                }
                            }

                            let mut entity_mut = self.world.entity_mut(entity);
                            entity_mut.get_mut::<IntentComponent>().unwrap().0 =
                                Some(AgentIntent {
                                    kind: intent_kind,
                                    target_agent: target_agent_id,
                                    target_semantic: Some(faction.agenda_tag.clone()),
                                    justification: format!(
                                        "Sobrescrita por aÃ§Ã£o fÃ­sica da facÃ§Ã£o '{}'",
                                        faction.name
                                    ),
                                    dominant_emotion: "furioso".to_string(),
                                    perceived_risk: 0,
                                    belief_updates: Vec::new(),
                                    priority: 255,
                                    social_move: None,
                                });
                            entity_mut.get_mut::<DestinationComponent>().unwrap().0 = Some(dest);
                            entity_mut.get_mut::<PathComponent>().unwrap().0.clear();
                        } else {
                            let path_opt = self.debug_find_path(current_pos, dest, Some(agent_id));
                            let mut entity_mut = self.world.entity_mut(entity);
                            entity_mut.get_mut::<IntentComponent>().unwrap().0 =
                                Some(AgentIntent {
                                    kind: IntentKind::Andar,
                                    target_agent: None,
                                    target_semantic: Some(faction.agenda_tag.clone()),
                                    justification: format!(
                                        "Caminhando para o motim da facÃ§Ã£o '{}'",
                                        faction.name
                                    ),
                                    dominant_emotion: "determinado".to_string(),
                                    perceived_risk: 0,
                                    belief_updates: Vec::new(),
                                    priority: 255,
                                    social_move: None,
                                });
                            entity_mut.get_mut::<DestinationComponent>().unwrap().0 = Some(dest);
                            if let Some(path) = path_opt {
                                entity_mut.get_mut::<PathComponent>().unwrap().0 = path;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn execute_food_riot_steal(&mut self, actor_id: u64) -> Result<()> {
        let current_pos = self.debug_agent_position(actor_id)?;
        let farm_building = self
            .spatial
            .buildings
            .iter()
            .filter(|b| b.kind == LocationKind::Farm)
            .min_by_key(|b| b.entrance.manhattan(current_pos))
            .cloned();

        if let Some(farm) = farm_building {
            if let Some(establishment) = self.establishment_by_building_mut(farm.id) {
                let grains_stolen = Self::take_resource(
                    &mut establishment.stock,
                    &ResourceKind::Graos.id().to_string(),
                    2,
                );

                if grains_stolen > 0 {
                    let entity = self.find_agent_entity(actor_id)?;
                    let mut entity_mut = self.world.entity_mut(entity);
                    let mut inventory = entity_mut.get_mut::<InventoryComponent>().unwrap();
                    Self::push_resource(
                        &mut inventory.0,
                        &ResourceKind::Graos.id().to_string(),
                        grains_stolen,
                    );
                    drop(entity_mut);

                    let mut factions = self.political_factions.clone();
                    for faction in &mut factions {
                        if faction.is_action_active && faction.member_ids.contains(&actor_id) {
                            if let Some(FactionObjective::FoodRiot {
                                grains_stolen: ref mut stolen,
                                ..
                            }) = faction.objective
                            {
                                *stolen += grains_stolen;
                            }
                        }
                    }
                    self.political_factions = factions;

                    let name = self.agent_name(actor_id)?;
                    self.push_event(WorldEvent {
                        day: self.day,
                        tick: self.tick_of_day,
                        actor: actor_id,
                        target: None,
                        kind: EventKind::Theft,
                        summary: format!(
                            "{} saqueia {} grÃ£os do Celeiro durante o motim!",
                            name, grains_stolen
                        ),
                        impact_tags: vec![
                            "politica".to_string(),
                            "motim_comida".to_string(),
                            "roubo".to_string(),
                        ],
                    });
                }
            }
        }
        Ok(())
    }

    pub(super) fn apply_faction_recruitment(
        &mut self,
        speaker_id: u64,
        listener_id: u64,
    ) -> Result<()> {
        let speaker_factions = self
            .political_factions
            .iter()
            .filter(|f| f.member_ids.contains(&speaker_id))
            .cloned()
            .collect::<Vec<_>>();

        for faction in speaker_factions {
            if faction.member_ids.contains(&listener_id) {
                continue;
            }

            let listener_in_any_faction = self
                .political_factions
                .iter()
                .any(|f| f.member_ids.contains(&listener_id));
            if listener_in_any_faction {
                continue;
            }

            let relation = self.relation_between(listener_id, speaker_id);
            let has_high_trust = relation.trust >= 10 || relation.friendship >= 10;

            let mut joins = has_high_trust;

            if !joins {
                match faction.objective {
                    Some(FactionObjective::FoodRiot { .. }) => {
                        let hunger = self.agent_state(listener_id)?.hunger;
                        if hunger >= 50 {
                            joins = true;
                        }
                    }
                    Some(FactionObjective::TaxBoycott { .. }) => {
                        if let Some(household_id) = self.household_id_for_agent(listener_id) {
                            if let Some(household) = self.household_by_id(household_id) {
                                if household.tax_arrears > 0 {
                                    joins = true;
                                }
                            }
                        }
                    }
                    Some(FactionObjective::DeposeLeader { .. }) => {
                        let profile = self.agent_profile(listener_id)?;
                        let is_rebel = profile.traits.contains(&"rebelde".to_string())
                            || profile.traits.contains(&"vingativo".to_string())
                            || profile.traits.contains(&"oportunista".to_string());
                        let chaos = self.agent_chaos_pressure(listener_id)?;
                        if is_rebel || chaos >= 50 {
                            joins = true;
                        }
                    }
                    Some(FactionObjective::VigilanteJustice {
                        suspect_agent_id, ..
                    }) => {
                        let resentment = self
                            .relation_between(listener_id, suspect_agent_id)
                            .resentment;
                        if resentment >= 15 {
                            joins = true;
                        }
                    }
                    None => {}
                }
            }

            if joins {
                let listener_influence = self.political_influence(listener_id);
                let speaker_name = self.agent_name(speaker_id)?;
                let listener_name = self.agent_name(listener_id)?;
                let faction_name = faction.name.clone();
                let faction_id = faction.id;
                let agenda_tag = faction.agenda_tag.clone();

                if let Some(f) = self
                    .political_factions
                    .iter_mut()
                    .find(|f| f.id == faction_id)
                {
                    f.member_ids.push(listener_id);
                    f.influence += listener_influence;
                }

                self.push_event(WorldEvent {
                    day: self.day,
                    tick: self.tick_of_day,
                    actor: speaker_id,
                    target: Some(listener_id),
                    kind: EventKind::FactionShift,
                    summary: format!(
                        "{} convence {} a se juntar Ã  facÃ§Ã£o '{}'.",
                        speaker_name, listener_name, faction_name
                    ),
                    impact_tags: vec!["politica".to_string(), "faccao".to_string(), agenda_tag],
                });
            }
        }
        Ok(())
    }

    pub(super) fn compute_chaos_pressure(
        state: &AgentState,
        profile: &AgentProfile,
        relations: &HashMap<u64, AgentRelation>,
        injury: &InjuryState,
        household_treasury: i32,
        food_crisis_level: u8,
    ) -> u32 {
        let max_resentment = relations.values().map(|r| r.resentment).max().unwrap_or(0);
        let trauma_count = profile.trauma_traits.len() as i32;

        let raw = (state.stress as f64 * 0.3)
            + (state.hunger as f64 * 0.25)
            + (max_resentment as f64 * 0.2)
            + (trauma_count as f64 * 8.0)
            + (injury.pain as f64 * 0.15)
            + if household_treasury <= 0 { 15.0 } else { 0.0 }
            + if food_crisis_level >= 2 { 10.0 } else { 0.0 };

        (raw as u32).min(100)
    }

    pub(super) fn apply_decretar_intent(
        &mut self,
        agent_id: u64,
        intent: &AgentIntent,
    ) -> Result<()> {
        let role_id = self.agent_role_id(agent_id)?;
        if role_id != "lider_local" {
            return Ok(());
        }

        let Some(ref edital_tag) = intent.target_semantic else {
            return Ok(());
        };

        if self.active_policy_act_by_agenda(edital_tag).is_some() {
            return Ok(());
        }

        let issue_id = self.next_political_issue_id;
        self.next_political_issue_id += 1;
        let policy_act_id = self.next_policy_act_id;
        self.next_policy_act_id += 1;

        let domain = Self::policy_domain_for_decree_tag(edital_tag);

        let summary = Self::edict_summary(edital_tag);
        let effects = Self::policy_effects_for_edict_tag(edital_tag);

        self.political_issues.push(PoliticalIssue {
            id: issue_id,
            agenda_tag: edital_tag.clone(),
            domain,
            proposed_value: "edital_rei".to_string(),
            summary: summary.clone(),
            proposed_by: Some(agent_id),
            support_score: 100,
            opposition_score: 0,
            supporter_ids: vec![agent_id],
            opposer_ids: Vec::new(),
            status: PoliticalIssueStatus::Open,
            opened_day: self.day,
            resolved_day: None,
        });

        self.policy_acts.push(PolicyAct {
            id: policy_act_id,
            agenda_tag: edital_tag.clone(),
            summary: summary.clone(),
            issuer_agent_id: Some(agent_id),
            issuer_polity_id: self.polities.first().map(|polity| polity.id),
            authority: PolicyAuthority::LocalLeader,
            scope: PolicyScope::GlobalVillage,
            target: PolicyTarget::None,
            effects,
            legitimacy: 55,
            enforcement: 65,
            resistance: 0,
            status: PolicyActStatus::Active,
            issued_day: self.day,
            issued_tick: self.tick_of_day,
            expires_day: None,
        });
        self.apply_decree_norm_change(edital_tag)?;

        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::PolicyProposal,
            summary: format!("O Lider Local decretou: {}", summary),
            impact_tags: vec![
                "politica".to_string(),
                "edital".to_string(),
                edital_tag.clone(),
            ],
        });

        self.add_memory(
            agent_id,
            MemoryKind::Fact,
            format!("Eu decretei o edital: {}", edital_tag),
            vec!["edital_rei".to_string(), edital_tag.clone()],
            5,
            vec![agent_id],
        )?;

        Ok(())
    }

    pub(super) fn apply_feudal_oath_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
    ) -> Result<()> {
        let Some(suzerain_id) = target_agent_id else {
            return Ok(());
        };
        if let Some(contract) = self
            .feudal_contracts
            .iter_mut()
            .find(|contract| contract.vassal_agent_id == agent_id)
        {
            contract.suzerain_agent_id = suzerain_id;
            contract.loyalty = (contract.loyalty + 12).clamp(0, 100);
            contract.perceived_legitimacy = (contract.perceived_legitimacy + 8).clamp(-100, 100);
            contract.status = FeudalContractStatus::Active;
        } else {
            let contract_id = self.next_feudal_contract_id;
            self.next_feudal_contract_id += 1;
            self.feudal_contracts.push(FeudalContract {
                id: contract_id,
                suzerain_agent_id: suzerain_id,
                vassal_agent_id: agent_id,
                territory_id: self.territory_for_agent(agent_id),
                holding_id: None,
                tribute_due_per_day: 1,
                levy_duty: 1,
                judicial_aid_duty: 0,
                maintenance_duty: 0,
                loyalty: 55,
                coercion: 20,
                perceived_legitimacy: 45,
                status: FeudalContractStatus::Active,
                last_updated_day: self.day,
            });
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: Some(suzerain_id),
            kind: EventKind::VassalOath,
            summary: format!(
                "{} jurou lealdade a {}.",
                self.agent_name(agent_id)?,
                self.agent_name(suzerain_id)?
            ),
            impact_tags: vec!["feudal".to_string(), "juramento".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_break_fealty_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
    ) -> Result<()> {
        for contract in self.feudal_contracts.iter_mut().filter(|contract| {
            contract.vassal_agent_id == agent_id
                && target_agent_id.is_none_or(|target| contract.suzerain_agent_id == target)
                && contract.status == FeudalContractStatus::Active
        }) {
            contract.status = FeudalContractStatus::Breached;
            contract.loyalty = (contract.loyalty - 25).clamp(0, 100);
            contract.last_updated_day = self.day;
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: target_agent_id,
            kind: EventKind::FeudalSanction,
            summary: format!(
                "{} rompeu ou enfraqueceu laços de lealdade feudal.",
                self.agent_name(agent_id)?
            ),
            impact_tags: vec!["feudal".to_string(), "ruptura".to_string()],
        });
        Ok(())
    }

    fn can_exercise_high_feudal_authority(&self, agent_id: u64) -> bool {
        self.agent_role_id(agent_id)
            .map(|role_id| role_id == "lider_local")
            .unwrap_or(false)
            || self
                .active_feudal_title_for_holder(agent_id)
                .is_some_and(|title| {
                    matches!(
                        title.rank,
                        FeudalRank::Rei
                            | FeudalRank::Duque
                            | FeudalRank::Conde
                            | FeudalRank::Barao
                            | FeudalRank::Senhor
                    )
                })
    }

    pub(super) fn apply_grant_title_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
        intent: &AgentIntent,
    ) -> Result<()> {
        if !self.can_exercise_high_feudal_authority(agent_id) {
            return Ok(());
        }
        let Some(holder_id) = target_agent_id else {
            return Ok(());
        };
        let title_index = self.feudal_titles.iter().position(|title| {
            title.active
                && title.holder_agent_id != Some(agent_id)
                && intent
                    .target_semantic
                    .as_ref()
                    .is_none_or(|target| title.name.to_lowercase().contains(&target.to_lowercase()))
        });
        let Some(title_index) = title_index else {
            return Ok(());
        };
        self.feudal_titles[title_index].holder_agent_id = Some(holder_id);
        self.feudal_titles[title_index].legitimacy =
            (self.feudal_titles[title_index].legitimacy + 5).clamp(0, 100);
        let actor_name = self.agent_name(agent_id)?;
        let holder_name = self.agent_name(holder_id)?;
        let title_name = self.feudal_titles[title_index].name.clone();
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: Some(holder_id),
            kind: EventKind::TitleGranted,
            summary: format!("{actor_name} concedeu o titulo {title_name} a {holder_name}."),
            impact_tags: vec!["feudal".to_string(), "titulo".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_revoke_title_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
        intent: &AgentIntent,
    ) -> Result<()> {
        if !self.can_exercise_high_feudal_authority(agent_id) {
            return Ok(());
        }
        let title_index = self.feudal_titles.iter().position(|title| {
            title.active
                && target_agent_id.is_none_or(|target| title.holder_agent_id == Some(target))
                && intent
                    .target_semantic
                    .as_ref()
                    .is_none_or(|target| title.name.to_lowercase().contains(&target.to_lowercase()))
        });
        let Some(title_index) = title_index else {
            return Ok(());
        };
        let holder = self.feudal_titles[title_index].holder_agent_id;
        self.feudal_titles[title_index].holder_agent_id = None;
        self.feudal_titles[title_index].legitimacy =
            (self.feudal_titles[title_index].legitimacy - 10).clamp(0, 100);
        let actor_name = self.agent_name(agent_id)?;
        let title_name = self.feudal_titles[title_index].name.clone();
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: holder,
            kind: EventKind::TitleRevoked,
            summary: format!("{actor_name} revogou o titulo {title_name}."),
            impact_tags: vec![
                "feudal".to_string(),
                "titulo".to_string(),
                "revogacao".to_string(),
            ],
        });
        Ok(())
    }

    pub(super) fn apply_appoint_office_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
        intent: &AgentIntent,
    ) -> Result<()> {
        if !self.can_exercise_high_feudal_authority(agent_id) {
            return Ok(());
        }
        let Some(holder_id) = target_agent_id else {
            return Ok(());
        };
        let office_index = self.authority_offices.iter().position(|office| {
            office.active
                && intent.target_semantic.as_ref().is_none_or(|target| {
                    office.name.to_lowercase().contains(&target.to_lowercase())
                })
        });
        let Some(office_index) = office_index else {
            return Ok(());
        };
        self.authority_offices[office_index].holder_agent_id = Some(holder_id);
        self.authority_offices[office_index].granter_agent_id = Some(agent_id);
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: Some(holder_id),
            kind: EventKind::TitleGranted,
            summary: format!(
                "{} nomeou {} para o ofício {}.",
                self.agent_name(agent_id)?,
                self.agent_name(holder_id)?,
                self.authority_offices[office_index].name
            ),
            impact_tags: vec!["feudal".to_string(), "oficio".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_demand_tribute_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
    ) -> Result<()> {
        let target_household_id =
            target_agent_id.and_then(|target_id| self.household_id_for_agent_immutable(target_id));
        let Some(household_id) = target_household_id else {
            return Ok(());
        };
        if let Some(household) = self
            .households
            .iter_mut()
            .find(|household| household.id == household_id)
        {
            household.feudal_tribute_due += 2;
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: target_agent_id,
            kind: EventKind::TributeDemanded,
            summary: format!("{} exigiu tributo adicional.", self.agent_name(agent_id)?),
            impact_tags: vec!["feudal".to_string(), "tributo".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_corvee_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
    ) -> Result<()> {
        let target_household_id =
            target_agent_id.and_then(|target_id| self.household_id_for_agent_immutable(target_id));
        let Some(household_id) = target_household_id else {
            return Ok(());
        };
        if let Some(household) = self
            .households
            .iter_mut()
            .find(|household| household.id == household_id)
        {
            household.corvee_days_due = (household.corvee_days_due + 2).clamp(0, 20);
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: target_agent_id,
            kind: EventKind::FeudalSanction,
            summary: format!(
                "{} cobrou dias extras de corveia.",
                self.agent_name(agent_id)?
            ),
            impact_tags: vec!["feudal".to_string(), "corveia".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_levy_call_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
    ) -> Result<()> {
        let target_household_id =
            target_agent_id.and_then(|target_id| self.household_id_for_agent_immutable(target_id));
        let Some(household_id) = target_household_id else {
            return Ok(());
        };
        if let Some(household) = self
            .households
            .iter_mut()
            .find(|household| household.id == household_id)
        {
            household.levy_service_due = (household.levy_service_due + 1).clamp(0, 10);
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: target_agent_id,
            kind: EventKind::LevyCalled,
            summary: format!("{} convocou levy feudal.", self.agent_name(agent_id)?),
            impact_tags: vec!["feudal".to_string(), "levy".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_recognize_heir_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
    ) -> Result<()> {
        let Some(target_id) = target_agent_id else {
            return Ok(());
        };
        if let Some(crisis) = self
            .succession_crises
            .iter_mut()
            .find(|crisis| crisis.status == SuccessionCrisisStatus::Open)
        {
            crisis.recognized_heir_id = Some(target_id);
            crisis.conflict_score = (crisis.conflict_score - 8).max(0);
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: Some(target_id),
                kind: EventKind::SuccessionRecognized,
                summary: format!(
                    "{} reconheceu {} como herdeiro legitimo.",
                    self.agent_name(agent_id)?,
                    self.agent_name(target_id)?
                ),
                impact_tags: vec!["feudal".to_string(), "sucessao".to_string()],
            });
        }
        Ok(())
    }

    pub(super) fn apply_support_claimant_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
    ) -> Result<()> {
        let Some(target_id) = target_agent_id else {
            return Ok(());
        };
        if let Some(crisis) = self
            .succession_crises
            .iter_mut()
            .find(|crisis| crisis.status == SuccessionCrisisStatus::Open)
        {
            if !crisis.claimant_ids.contains(&target_id) {
                crisis.claimant_ids.push(target_id);
            }
            crisis.conflict_score = (crisis.conflict_score + 6).clamp(0, 100);
            self.push_event(WorldEvent {
                day: self.day,
                tick: self.tick_of_day,
                actor: agent_id,
                target: Some(target_id),
                kind: EventKind::SuccessionContested,
                summary: format!(
                    "{} apoiou a pretensão de {}.",
                    self.agent_name(agent_id)?,
                    self.agent_name(target_id)?
                ),
                impact_tags: vec![
                    "feudal".to_string(),
                    "sucessao".to_string(),
                    "pretendente".to_string(),
                ],
            });
        }
        Ok(())
    }

    pub(super) fn apply_usurp_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
    ) -> Result<()> {
        let Some(target_id) = target_agent_id else {
            return Ok(());
        };
        let target_title_id = self
            .active_feudal_title_for_holder(target_id)
            .map(|title| title.id);
        let Some(title_id) = target_title_id else {
            return Ok(());
        };
        if self.feudal_power_for_agent(agent_id) + 10 < self.feudal_power_for_agent(target_id) {
            return Ok(());
        }
        if let Some(crisis) = self.succession_crises.iter_mut().find(|crisis| {
            crisis.title_id == title_id && crisis.status == SuccessionCrisisStatus::Open
        }) {
            crisis.usurper_id = Some(agent_id);
            if !crisis.claimant_ids.contains(&agent_id) {
                crisis.claimant_ids.push(agent_id);
            }
            crisis.conflict_score = (crisis.conflict_score + 18).clamp(0, 100);
        } else {
            let title = self
                .feudal_titles
                .iter()
                .find(|title| title.id == title_id)
                .cloned()
                .ok_or_else(|| anyhow!("titulo feudal inexistente para usurpacao"))?;
            self.open_succession_crisis_for_title(&title)?;
            if let Some(crisis) = self.succession_crises.iter_mut().find(|crisis| {
                crisis.title_id == title_id && crisis.status == SuccessionCrisisStatus::Open
            }) {
                crisis.usurper_id = Some(agent_id);
                if !crisis.claimant_ids.contains(&agent_id) {
                    crisis.claimant_ids.push(agent_id);
                }
            }
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: Some(target_id),
            kind: EventKind::Usurpation,
            summary: format!(
                "{} iniciou manobra de usurpação contra {}.",
                self.agent_name(agent_id)?,
                self.agent_name(target_id)?
            ),
            impact_tags: vec!["feudal".to_string(), "usurpacao".to_string()],
        });
        Ok(())
    }

    pub(super) fn apply_claim_territory_intent(
        &mut self,
        agent_id: u64,
        intent: &AgentIntent,
    ) -> Result<()> {
        let Some(ref target_semantic) = intent.target_semantic else {
            return Ok(());
        };
        let territory_id = self
            .place_by_id(target_semantic)
            .and_then(|place| place.territory_id);
        let Some(territory_id) = territory_id else {
            return Ok(());
        };
        let polity_id = self.polities.first().map(|polity| polity.id);
        let Some(polity_id) = polity_id else {
            return Ok(());
        };
        if let Some(territory) = self
            .territories
            .iter_mut()
            .find(|territory| territory.id == territory_id)
        {
            if !territory.claimed_by.contains(&polity_id) {
                territory.claimed_by.push(polity_id);
            }
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: None,
            kind: EventKind::InstitutionalDispute,
            summary: format!(
                "{} reforçou reivindicação territorial sobre {}.",
                self.agent_name(agent_id)?,
                target_semantic
            ),
            impact_tags: vec![
                "feudal".to_string(),
                "territorio".to_string(),
                "reivindicacao".to_string(),
            ],
        });
        Ok(())
    }

    pub(super) fn apply_negotiate_suzerainty_intent(
        &mut self,
        agent_id: u64,
        target_agent_id: Option<u64>,
    ) -> Result<()> {
        let Some(target_id) = target_agent_id else {
            return Ok(());
        };
        if let Some(contract) = self.feudal_contracts.iter_mut().find(|contract| {
            (contract.vassal_agent_id == agent_id && contract.suzerain_agent_id == target_id)
                || (contract.vassal_agent_id == target_id && contract.suzerain_agent_id == agent_id)
        }) {
            contract.loyalty = (contract.loyalty + 6).clamp(0, 100);
            contract.coercion = (contract.coercion - 4).clamp(0, 100);
            contract.perceived_legitimacy = (contract.perceived_legitimacy + 5).clamp(-100, 100);
            contract.last_updated_day = self.day;
        }
        self.push_event(WorldEvent {
            day: self.day,
            tick: self.tick_of_day,
            actor: agent_id,
            target: Some(target_id),
            kind: EventKind::InstitutionalDispute,
            summary: format!(
                "{} negociou termos de suserania com {}.",
                self.agent_name(agent_id)?,
                self.agent_name(target_id)?
            ),
            impact_tags: vec!["feudal".to_string(), "suserania".to_string()],
        });
        Ok(())
    }
}
