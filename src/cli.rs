use crate::headless::HeadlessConfig;
use crate::sim_core::SimulationConfig;
use anyhow::{Result, anyhow, bail};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunMode {
    Gui,
    Headless(HeadlessConfig),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOptions {
    pub db_path: PathBuf,
    pub force_new: bool,
    pub simulation: SimulationConfig,
    pub mode: RunMode,
}

#[derive(Debug)]
pub enum CliCommand {
    Run(CliOptions),
    Help,
}

pub fn parse_cli_args<I>(args: I) -> Result<CliCommand>
where
    I: IntoIterator<Item = String>,
{
    let mut db_path = env::var("VILLAGE_DB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("village_sim.sqlite"));
    let mut force_new = false;
    let mut simulation = SimulationConfig::default();
    let mut headless = HeadlessConfig::default();
    let mut headless_enabled = false;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(CliCommand::Help),
            "--headless" => headless_enabled = true,
            "--gui" => headless_enabled = false,
            "--new" => force_new = true,
            "--map" => headless.render_map = true,
            "--db" => {
                db_path = PathBuf::from(next_string(&mut args, "--db")?);
            }
            "--ticks" => {
                let value: u64 = next_parsed(&mut args, "--ticks")?;
                if value == 0 {
                    bail!("--ticks deve ser maior que zero");
                }
                headless.max_ticks = Some(value);
            }
            "--days" => {
                let value: u32 = next_parsed(&mut args, "--days")?;
                if value == 0 {
                    bail!("--days deve ser maior que zero");
                }
                headless.max_days = Some(value);
            }
            "--save-every" => {
                let value: u64 = next_parsed(&mut args, "--save-every")?;
                headless.save_every_ticks = if value == 0 { None } else { Some(value) };
            }
            "--summary-every" => {
                headless.summary_every_ticks = next_parsed(&mut args, "--summary-every")?;
            }
            "--event-tail" => {
                headless.event_tail = next_parsed(&mut args, "--event-tail")?;
            }
            "--ticks-per-second" => {
                headless.ticks_per_second = next_parsed(&mut args, "--ticks-per-second")?;
            }
            "--seed" => {
                simulation.world_seed = next_parsed(&mut args, "--seed")?;
            }
            "--agents" | "--population" => {
                simulation.max_agents = next_parsed(&mut args, "--agents | --population")?;
            }
            "--grid-width" | "--width" => {
                simulation.grid_width = next_parsed(&mut args, "--grid-width | --width")?;
            }
            "--grid-height" | "--height" => {
                simulation.grid_height = next_parsed(&mut args, "--grid-height | --height")?;
            }
            "--num-villages" => {
                simulation.num_villages = next_parsed(&mut args, "--num-villages")?;
            }
            "--history-years" => {
                simulation.history_years = next_parsed(&mut args, "--history-years")?;
            }
            "--history-founding-households" => {
                simulation.history_founding_households =
                    next_parsed(&mut args, "--history-founding-households")?;
            }
            "--history-seed" => {
                simulation.history_seed = Some(next_parsed(&mut args, "--history-seed")?);
            }
            "--village-name" => {
                simulation.village_name = next_string(&mut args, "--village-name")?;
            }
            other => bail!("argumento desconhecido: {other}"),
        }
    }

    if headless.summary_every_ticks == 0 {
        bail!("--summary-every deve ser maior que zero");
    }
    if headless.event_tail == 0 {
        bail!("--event-tail deve ser maior que zero");
    }
    if headless.ticks_per_second == 0 {
        bail!("--ticks-per-second deve ser maior que zero");
    }
    if simulation.max_agents == 0 {
        bail!("--agents deve ser maior que zero");
    }
    if simulation.num_villages == 0 {
        bail!("--num-villages deve ser maior que zero");
    }
    if simulation.history_years == 0 {
        bail!("--history-years deve ser maior que zero");
    }
    if simulation.history_founding_households == 0 {
        bail!("--history-founding-households deve ser maior que zero");
    }
    if simulation.ticks_per_day == 0 {
        bail!("ticks_per_day interno deve ser maior que zero");
    }
    if simulation.grid_width < 100 || simulation.grid_height < 60 {
        bail!("Dimensoes do grid devem ser de pelo menos 100x60");
    }

    let mode = if headless_enabled {
        RunMode::Headless(headless)
    } else {
        RunMode::Gui
    };

    Ok(CliCommand::Run(CliOptions {
        db_path,
        force_new,
        simulation,
        mode,
    }))
}

pub fn usage() -> &'static str {
    concat!(
        "Uso:\n",
        "  medieval_village_llm [--headless] [opcoes]\n\n",
        "Modos:\n",
        "  --gui                   interface grafica 2D pixel art (padrao)\n",
        "  --headless              modo console, imprime relatorios no stdout\n\n",
        "Bootstrap:\n",
        "  --new                   ignora save existente e cria uma vila nova\n",
        "  --db PATH               sobrescreve VILLAGE_DB_PATH\n",
        "  --village-name NOME     nome da vila ao criar um mundo novo\n",
        "  --seed N                seed do gerador espacial\n",
        "  --agents, --population N quantidade de agentes iniciais\n",
        "  --grid-width, --width N  largura do grid\n",
        "  --grid-height, --height N altura do grid\n",
        "  --num-villages N        quantidade de vilas a gerar\n\n",
        "  --history-years N       anos da pre-historia deterministica\n",
        "  --history-founding-households N casas fundadoras iniciais\n",
        "  --history-seed N        seed dedicada da pre-historia (padrao: --seed)\n\n",
        "Headless:\n",
        "  --ticks N               encerra apos N ticks executados neste processo\n",
        "  --days N                encerra apos N dias simulados neste processo\n",
        "  --save-every N          salva checkpoint intervalar a cada N ticks (0 desativa)\n",
        "  --summary-every N       imprime relatorio a cada N ticks\n",
        "  --event-tail N          quantos eventos recentes mostrar por relatorio\n",
        "  --ticks-per-second N    ritmo real da simulacao no headless (padrao: 1)\n",
        "  --map                   inclui o mapa ASCII completo nos relatorios\n",
        "  --help                  mostra esta ajuda\n",
    )
}

fn next_string<I>(args: &mut I, flag: &str) -> Result<String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .ok_or_else(|| anyhow!("faltou valor para {}", flag))
}

fn next_parsed<T, I>(args: &mut I, flag: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
    I: Iterator<Item = String>,
{
    let raw = next_string(args, flag)?;
    raw.parse::<T>()
        .map_err(|error| anyhow!("valor invalido para {}: {} ({})", flag, raw, error))
}

#[cfg(test)]
mod tests {
    use super::{CliCommand, RunMode, parse_cli_args};
    use crate::sim_core::DEFAULT_TICKS_PER_DAY;

    #[test]
    fn parses_headless_options_and_bootstrap() {
        let command = parse_cli_args([
            "--headless".to_string(),
            "--new".to_string(),
            "--ticks".to_string(),
            "48".to_string(),
            "--days".to_string(),
            "2".to_string(),
            "--save-every".to_string(),
            "12".to_string(),
            "--summary-every".to_string(),
            "6".to_string(),
            "--event-tail".to_string(),
            "4".to_string(),
            "--ticks-per-second".to_string(),
            "3".to_string(),
            "--map".to_string(),
            "--seed".to_string(),
            "77".to_string(),
            "--agents".to_string(),
            "18".to_string(),
            "--grid-width".to_string(),
            "150".to_string(),
            "--grid-height".to_string(),
            "100".to_string(),
            "--history-years".to_string(),
            "80".to_string(),
            "--history-founding-households".to_string(),
            "4".to_string(),
            "--history-seed".to_string(),
            "991".to_string(),
            "--village-name".to_string(),
            "Pedra Clara".to_string(),
        ])
        .expect("cli parse should succeed");

        let CliCommand::Run(options) = command else {
            panic!("expected run command");
        };
        assert!(options.force_new);
        assert_eq!(options.simulation.world_seed, 77);
        assert_eq!(options.simulation.max_agents, 18);
        assert_eq!(options.simulation.ticks_per_day, DEFAULT_TICKS_PER_DAY);
        assert_eq!(options.simulation.grid_width, 150);
        assert_eq!(options.simulation.grid_height, 100);
        assert_eq!(options.simulation.history_years, 80);
        assert_eq!(options.simulation.history_founding_households, 4);
        assert_eq!(options.simulation.history_seed, Some(991));
        assert_eq!(options.simulation.village_name, "Pedra Clara");
        match options.mode {
            RunMode::Headless(headless) => {
                assert_eq!(headless.max_ticks, Some(48));
                assert_eq!(headless.max_days, Some(2));
                assert_eq!(headless.save_every_ticks, Some(12));
                assert_eq!(headless.summary_every_ticks, 6);
                assert_eq!(headless.event_tail, 4);
                assert_eq!(headless.ticks_per_second, 3);
                assert!(headless.render_map);
            }
            RunMode::Gui => panic!("expected headless mode"),
        }
    }

    #[test]
    fn rejects_unknown_argument() {
        let error = parse_cli_args(["--nao-existe".to_string()]).expect_err("must fail");
        assert!(error.to_string().contains("argumento desconhecido"));
    }

    #[test]
    fn rejects_zero_ticks_and_disables_zero_save_interval() {
        let ticks_error = parse_cli_args([
            "--headless".to_string(),
            "--ticks".to_string(),
            "0".to_string(),
        ])
        .expect_err("zero ticks must fail");
        assert!(
            ticks_error
                .to_string()
                .contains("--ticks deve ser maior que zero")
        );

        let tick_rate_error = parse_cli_args([
            "--headless".to_string(),
            "--ticks-per-second".to_string(),
            "0".to_string(),
        ])
        .expect_err("zero tick rate must fail");
        assert!(
            tick_rate_error
                .to_string()
                .contains("--ticks-per-second deve ser maior que zero")
        );

        let command = parse_cli_args([
            "--headless".to_string(),
            "--save-every".to_string(),
            "0".to_string(),
        ])
        .expect("save interval zero should disable interval saves");
        let CliCommand::Run(options) = command else {
            panic!("expected run command");
        };
        match options.mode {
            RunMode::Headless(headless) => assert_eq!(headless.save_every_ticks, None),
            RunMode::Gui => panic!("expected headless mode"),
        }
    }
}
