use anyhow::Result;
use medieval_village_llm::cli::{CliCommand, RunMode, parse_cli_args, usage};
use medieval_village_llm::headless::run_headless;
use medieval_village_llm::llm_adapter::adapter_from_env;
use medieval_village_llm::persistence::Persistence;
use medieval_village_llm::sim_core::Simulation;
use medieval_village_llm::tui::run_tui;
use std::env;

fn main() -> Result<()> {
    let command = parse_cli_args(env::args().skip(1))?;
    let options = match command {
        CliCommand::Help => {
            println!("{}", usage());
            return Ok(());
        }
        CliCommand::Run(options) => options,
    };

    let persistence = Persistence::open(&options.db_path)?;
    let simulation = if options.force_new {
        Simulation::seeded(options.simulation.clone())
    } else {
        match persistence.load_latest()? {
            Some(snapshot) => Simulation::from_snapshot(snapshot),
            None => Simulation::seeded(options.simulation.clone()),
        }
    };
    let llm = adapter_from_env()?;
    match options.mode {
        RunMode::Tui => run_tui(simulation, llm, persistence),
        RunMode::Headless(config) => run_headless(simulation, llm, persistence, config),
    }
}
