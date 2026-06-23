use super::*;

impl Simulation {
    pub fn debug_agent_position(&mut self, agent_id: u64) -> Result<TileCoord> {
        let entity = self.find_agent_entity(agent_id)?;
        Ok(self
            .world
            .entity(entity)
            .get::<PositionComponent>()
            .ok_or_else(|| anyhow!("missing position component"))?
            .0)
    }

    pub fn debug_force_agent_position(&mut self, agent_id: u64, coord: TileCoord) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<PositionComponent>()
            .ok_or_else(|| anyhow!("missing position component"))?
            .0 = coord;
        self.world
            .entity_mut(entity)
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
        self.world
            .entity_mut(entity)
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        Ok(())
    }

    pub fn debug_force_agent_state(&mut self, agent_id: u64, state: AgentState) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<StateComponent>()
            .ok_or_else(|| anyhow!("missing state component"))?
            .0 = state;
        Ok(())
    }

    pub fn debug_assign_intent(&mut self, agent_id: u64, intent: AgentIntent) -> Result<()> {
        self.pending_thoughts
            .retain(|pending| pending.agent_id != agent_id);
        self.clear_active_economic_task(agent_id)?;
        for task in self.economic_tasks.iter_mut().filter(|task| {
            task.assigned_agent_id == Some(agent_id)
                && !matches!(
                    task.phase,
                    EconomicTaskPhase::Completed | EconomicTaskPhase::Failed
                )
        }) {
            task.assigned_agent_id = None;
        }
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<IntentComponent>()
            .ok_or_else(|| anyhow!("missing intent component"))?
            .0 = Some(intent.clone());
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = None;
        entity_mut
            .get_mut::<DestinationLabelComponent>()
            .ok_or_else(|| anyhow!("missing destination label component"))?
            .0 = intent.target_semantic.clone();
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0
            .clear();
        entity_mut
            .get_mut::<EconomicActivityComponent>()
            .ok_or_else(|| anyhow!("missing economy component"))?
            .active_task_id = None;
        Ok(())
    }

    pub fn debug_apply_think_maker_output(
        &mut self,
        agent_id: u64,
        output: ThinkMakerOutput,
    ) -> Result<()> {
        self.apply_think_maker_output(agent_id, output)
    }

    pub fn debug_remove_all_beds(&mut self) {
        self.spatial
            .fixtures
            .retain(|fixture| fixture.kind != FixtureKind::Bed);
    }

    pub fn debug_force_navigation(
        &mut self,
        agent_id: u64,
        destination: TileCoord,
        path: Vec<TileCoord>,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entity_mut = self.world.entity_mut(entity);
        entity_mut
            .get_mut::<DestinationComponent>()
            .ok_or_else(|| anyhow!("missing destination component"))?
            .0 = Some(destination);
        entity_mut
            .get_mut::<DestinationLabelComponent>()
            .ok_or_else(|| anyhow!("missing destination label component"))?
            .0 = Some("debug".to_string());
        entity_mut
            .get_mut::<PathComponent>()
            .ok_or_else(|| anyhow!("missing path component"))?
            .0 = path;
        Ok(())
    }

    pub fn debug_find_path(
        &mut self,
        start: TileCoord,
        goal: TileCoord,
        ignore_agent_id: Option<u64>,
    ) -> Option<Vec<TileCoord>> {
        self.find_path(start, goal, ignore_agent_id)
    }

    pub fn debug_try_social(
        &mut self,
        actor_id: u64,
        target_id: u64,
        _llm: &dyn LlmAdapter,
    ) -> Result<bool> {
        if !self.agents_adjacent(actor_id, target_id)? {
            return Ok(false);
        }
        self.open_conversation(
            vec![actor_id, target_id],
            SocialMove::Chat,
            "contato direto",
        )
    }

    pub fn debug_add_memory(
        &mut self,
        agent_id: u64,
        kind: MemoryKind,
        summary: String,
        tags: Vec<String>,
        weight: i32,
        about: Vec<u64>,
    ) -> Result<()> {
        self.add_memory(agent_id, kind, summary, tags, weight, about)
    }

    pub fn debug_set_relation(
        &mut self,
        agent_id: u64,
        other_id: u64,
        relation: AgentRelation,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        self.world
            .entity_mut(entity)
            .get_mut::<RelationComponent>()
            .ok_or_else(|| anyhow!("missing relation component"))?
            .0
            .insert(other_id, relation);
        Ok(())
    }

    pub fn debug_set_lineage(
        &mut self,
        agent_id: u64,
        age: u32,
        gender: String,
        spouse: Option<u64>,
        parents: Vec<u64>,
        children: Vec<u64>,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entry = self.world.entity_mut(entity);
        let mut lineage = entry
            .get_mut::<LineageComponent>()
            .ok_or_else(|| anyhow!("missing lineage component"))?;
        lineage.age = age;
        lineage.gender = gender;
        lineage.spouse = spouse;
        lineage.parents = parents;
        lineage.children = children;
        Ok(())
    }

    pub fn debug_advance_day(&mut self) -> Result<()> {
        self.close_daily_economy()?;
        self.apply_daily_aging()?;
        self.apply_daily_births()?;
        self.apply_daily_marriages()?;
        self.update_mourning_states()?;
        self.generate_daily_caravans()?;
        self.day += 1;
        self.tick_of_day = 0;
        Ok(())
    }

    pub fn debug_kill_agent(&mut self, agent_id: u64, reason: &str) -> Result<()> {
        self.mark_agent_dead(agent_id, reason)
    }

    pub fn debug_set_agent_cash(&mut self, agent_id: u64, amount: i32) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut entry = self.world.entity_mut(entity);
        let mut inv = entry
            .get_mut::<InventoryComponent>()
            .ok_or_else(|| anyhow!("missing inventory component"))?;
        if let Some(money_stack) = inv
            .0
            .iter_mut()
            .find(|stack| stack.resource_id == ResourceKind::Moedas.id())
        {
            money_stack.amount = amount;
        } else {
            inv.0.push(ResourceStack {
                resource_id: ResourceKind::Moedas.id().to_string(),
                amount,
            });
        }
        Ok(())
    }

    pub fn debug_set_household_members(
        &mut self,
        household_id: BuildingId,
        member_ids: Vec<u64>,
    ) -> Result<()> {
        let Some(household) = self.households.iter_mut().find(|h| h.id == household_id) else {
            return Err(anyhow!("household {household_id} not found"));
        };
        household.member_ids = member_ids;
        Ok(())
    }

    pub fn debug_set_public_treasury(&mut self, amount: i32) {
        self.village_economy.public_treasury = amount.max(0);
    }

    pub fn debug_set_household_treasury(
        &mut self,
        household_id: BuildingId,
        amount: i32,
    ) -> Result<()> {
        let Some(household) = self.household_by_id_mut(household_id) else {
            return Err(anyhow!("household {household_id} not found"));
        };
        household.treasury = amount.max(0);
        Ok(())
    }

    pub fn debug_set_establishment_cash(
        &mut self,
        establishment_id: EstablishmentId,
        amount: i32,
    ) -> Result<()> {
        let Some(establishment) = self.establishment_by_id_mut(establishment_id) else {
            return Err(anyhow!("establishment {establishment_id} not found"));
        };
        establishment.cash = amount.max(0);
        Ok(())
    }

    pub fn debug_clear_household_food(&mut self, household_id: BuildingId) -> Result<()> {
        let Some(household) = self.household_by_id_mut(household_id) else {
            return Err(anyhow!("household {household_id} not found"));
        };
        household.pantry.clear();
        household.reserved_food.clear();
        Ok(())
    }

    pub fn debug_refresh_economy(&mut self) -> Result<()> {
        self.refresh_economy_state()?;
        self.ensure_economic_tasks();
        Ok(())
    }

    pub fn debug_set_household_tax_arrears(
        &mut self,
        household_id: BuildingId,
        arrears: i32,
    ) -> Result<()> {
        let Some(household) = self.household_by_id_mut(household_id) else {
            return Err(anyhow!("household {household_id} not found"));
        };
        household.tax_arrears = arrears.max(0);
        Ok(())
    }

    pub fn debug_refresh_politics(&mut self) -> Result<()> {
        self.refresh_political_state()
    }

    pub fn debug_resolve_daily_politics(&mut self) -> Result<()> {
        self.resolve_daily_politics()
    }

    pub fn debug_activate_faction(&mut self, faction_id: PoliticalFactionId) -> Result<()> {
        if let Some(faction) = self
            .political_factions
            .iter_mut()
            .find(|f| f.id == faction_id)
        {
            faction.is_action_active = true;
            faction.rage = 50;
            Ok(())
        } else {
            Err(anyhow!("faction not found"))
        }
    }

    pub fn debug_add_establishment_stock(
        &mut self,
        building_id: BuildingId,
        resource_id: &str,
        amount: i32,
    ) -> Result<()> {
        if let Some(establishment) = self
            .establishments
            .iter_mut()
            .find(|e| e.building_id == Some(building_id))
        {
            Self::push_resource(&mut establishment.stock, resource_id, amount);
            Ok(())
        } else {
            Err(anyhow!(
                "establishment not found for building {}",
                building_id
            ))
        }
    }

    pub fn debug_clear_establishment_stock(&mut self, building_id: BuildingId) -> Result<()> {
        if let Some(establishment) = self
            .establishments
            .iter_mut()
            .find(|e| e.building_id == Some(building_id))
        {
            establishment.stock.clear();
            Ok(())
        } else {
            Err(anyhow!(
                "establishment not found for building {}",
                building_id
            ))
        }
    }

    pub fn debug_execute_economic_transfer(
        &mut self,
        sender_id: u64,
        transfer: &crate::agent_mind::EconomicTransfer,
    ) -> Result<()> {
        self.execute_dialogue_economic_transfer(sender_id, transfer)
    }

    pub fn debug_execute_secret_reveal(
        &mut self,
        sender_id: u64,
        reveal: &crate::agent_mind::RevealedSecret,
    ) -> Result<()> {
        self.execute_dialogue_secret_reveal(sender_id, reveal)
    }

    pub fn debug_execute_make_promise(
        &mut self,
        sender_id: u64,
        promise: &crate::agent_mind::ProposedPromise,
    ) -> Result<()> {
        self.execute_dialogue_make_promise(sender_id, promise)
    }

    pub fn debug_create_crime_case(
        &mut self,
        crime_type: CrimeType,
        victim_id: Option<u64>,
        suspect_id: Option<u64>,
        severity: u8,
    ) -> Result<u64> {
        let id = self.next_crime_case_id;
        self.next_crime_case_id += 1;
        self.crime_cases.push(CrimeCase {
            id,
            crime_type,
            victim_id,
            suspect_id,
            witnesses: vec![],
            evidence: vec![],
            severity,
            confidence: 30,
            status: CrimeCaseStatus::Open,
            sentence: SentenceKind::None,
            opened_day: self.day,
            opened_tick: self.tick_of_day,
            summary: format!(
                "Crime {:?} de suspect={:?} contra victim={:?}",
                crime_type, suspect_id, victim_id
            ),
        });
        if let Some(suspect) = suspect_id {
            self.generate_crime_secret(id, suspect, victim_id, &[], false)?;
        }
        Ok(id)
    }

    pub fn debug_world_mut(&mut self) -> &mut bevy_ecs::prelude::World {
        &mut self.world
    }

    pub fn debug_set_agent_work_building(
        &mut self,
        agent_id: u64,
        building_id: Option<BuildingId>,
    ) -> Result<()> {
        let entity = self.find_agent_entity(agent_id)?;
        let mut query = self.world.query::<&mut AgentCore>();
        if let Ok(mut core) = query.get_mut(&mut self.world, entity) {
            core.work_building_id = building_id;
            Ok(())
        } else {
            Err(anyhow!("agent core not found"))
        }
    }

    pub fn debug_set_establishment_owner_household(
        &mut self,
        establishment_id: EstablishmentId,
        owner_household_id: BuildingId,
    ) -> Result<()> {
        if let Some(est) = self
            .establishments
            .iter_mut()
            .find(|e| e.id == establishment_id)
        {
            est.owner_household_ids = vec![owner_household_id];
            est.public_service = false;
            Ok(())
        } else {
            Err(anyhow!("establishment not found"))
        }
    }

    pub fn debug_add_establishment_to_estate_holding(
        &mut self,
        suzerain_id: u64,
        establishment_id: EstablishmentId,
    ) -> Result<()> {
        if let Some(holding) = self
            .estate_holdings
            .iter_mut()
            .find(|h| h.holder_agent_id == Some(suzerain_id))
        {
            holding.establishment_ids.push(establishment_id);
            Ok(())
        } else {
            Err(anyhow!("estate holding not found"))
        }
    }

    pub fn debug_recalculate_territory_values(&mut self) {
        self.recalculate_territory_values();
    }

    pub fn debug_apply_daily_organic_expansion(&mut self) {
        self.apply_daily_organic_expansion();
    }

    pub fn debug_generate_emergent_polity_name(&self, founder_id: u64) -> String {
        self.generate_emergent_polity_name(founder_id)
    }

    pub fn debug_execute_construction_material_task(
        &mut self,
        agent_id: u64,
        task: EconomicTask,
        project_id: u64,
    ) -> Result<()> {
        self.execute_construction_material_task(agent_id, task, project_id)
    }

    pub fn debug_materialize_construction_project(
        &mut self,
        project: &ConstructionProject,
    ) -> Option<BuildingId> {
        self.materialize_construction_project(project)
    }

    pub fn debug_territories(&self) -> &[Territory] {
        &self.territories
    }

    pub fn debug_territories_mut(&mut self) -> &mut Vec<Territory> {
        &mut self.territories
    }

    pub fn debug_polities(&self) -> &[Polity] {
        &self.polities
    }

    pub fn debug_polities_mut(&mut self) -> &mut Vec<Polity> {
        &mut self.polities
    }

    pub fn debug_wars_mut(&mut self) -> &mut Vec<WarState> {
        &mut self.wars
    }

    pub fn debug_wars(&self) -> &[WarState] {
        &self.wars
    }

    pub fn debug_next_construction_project_id(&self) -> u64 {
        self.next_construction_project_id
    }

    pub fn debug_set_next_construction_project_id(&mut self, val: u64) {
        self.next_construction_project_id = val;
    }

    pub fn debug_construction_projects_mut(&mut self) -> &mut Vec<ConstructionProject> {
        &mut self.construction_projects
    }

    pub fn debug_establishments_mut(&mut self) -> &mut Vec<EstablishmentEconomy> {
        &mut self.establishments
    }

    pub fn debug_establishments(&self) -> &[EstablishmentEconomy] {
        &self.establishments
    }

    pub fn debug_spawn_creature(
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
        self.spawn_creature_entity(
            id,
            name,
            species,
            is_legendary,
            position,
            habitat_territory_id,
            max_health,
            attack_power,
        );
    }

    pub fn debug_is_creature(&self, id: u64) -> bool {
        self.is_creature(id)
    }

    pub fn debug_creature_health(&self, id: u64) -> Result<i32> {
        let ent = self.find_creature_entity(id)?;
        Ok(self
            .world
            .entity(ent)
            .get::<CreatureStateComponent>()
            .unwrap()
            .health)
    }

    pub fn debug_creature_position(&self, id: u64) -> Result<TileCoord> {
        self.creature_position(id)
    }

    pub fn debug_creatures(&mut self) -> Vec<Creature> {
        let mut list = Vec::new();
        let mut creature_query = self.world.query::<(
            &CreatureCore,
            &CreatureStateComponent,
            &InjuryComponent,
            &PositionComponent,
            &LifeStatusComponent,
        )>();
        for (core, state, injury, position, life) in creature_query.iter(&self.world) {
            list.push(Creature {
                id: core.id,
                name: core.name.clone(),
                species: core.species.clone(),
                is_legendary: core.is_legendary,
                health: state.health,
                max_health: state.max_health,
                attack_power: state.attack_power,
                position: position.0,
                habitat_territory_id: core.habitat_territory_id,
                active: life.0 == AgentLifeStatus::Vivo,
                injury: injury.0.clone(),
            });
        }
        list
    }

    pub fn debug_apply_attack_on_creature(
        &mut self,
        actor_id: u64,
        target_creature_id: u64,
        continuing_combat: bool,
    ) -> Result<()> {
        self.apply_attack_on_creature(actor_id, target_creature_id, continuing_combat)
    }

    pub fn debug_tick_fauna_behavior(&mut self) -> Result<()> {
        self.tick_fauna_behavior()
    }

    pub fn debug_hunting_quests(&self) -> &Vec<HuntingQuest> {
        &self.hunting_quests
    }

    pub fn debug_hunting_quests_mut(&mut self) -> &mut Vec<HuntingQuest> {
        &mut self.hunting_quests
    }
}
