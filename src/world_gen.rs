use crate::economy_catalog::{default_economy_catalog, validate_catalog};
use crate::sim_core::SimulationConfig;
use crate::world_model::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl Rect {
    fn border_tiles(self) -> Vec<TileCoord> {
        let mut tiles = Vec::new();
        for y in self.y1..=self.y2 {
            for x in self.x1..=self.x2 {
                if x == self.x1 || x == self.x2 || y == self.y1 || y == self.y2 {
                    tiles.push(TileCoord { x, y });
                }
            }
        }
        tiles
    }

    fn interior_tiles(self) -> Vec<TileCoord> {
        let mut tiles = Vec::new();
        for y in (self.y1 + 1)..self.y2 {
            for x in (self.x1 + 1)..self.x2 {
                tiles.push(TileCoord { x, y });
            }
        }
        tiles
    }

    fn footprint(self) -> Vec<TileCoord> {
        let mut tiles = Vec::new();
        for y in self.y1..=self.y2 {
            for x in self.x1..=self.x2 {
                tiles.push(TileCoord { x, y });
            }
        }
        tiles
    }
}

#[derive(Clone)]
struct FixturePlacement {
    kind: FixtureKind,
    coord: TileCoord,
    name: &'static str,
    blocks_movement: bool,
    stock: Vec<ResourceStack>,
}

fn fixture(
    kind: FixtureKind,
    x: i32,
    y: i32,
    name: &'static str,
    blocks_movement: bool,
    stock: Vec<ResourceStack>,
) -> FixturePlacement {
    FixturePlacement {
        kind,
        coord: TileCoord { x, y },
        name,
        blocks_movement,
        stock,
    }
}

struct SpatialBuilder {
    grid: WorldGrid,
    buildings: Vec<BuildingSpec>,
    rooms: Vec<RoomSpec>,
    fixtures: Vec<FixtureSpec>,
    next_building_id: u64,
    next_room_id: u64,
    next_fixture_id: u64,
}

impl SpatialBuilder {
    fn new(width: i32, height: i32) -> Self {
        Self {
            grid: WorldGrid {
                width,
                height,
                tiles: Vec::new(),
            },
            buildings: Vec::new(),
            rooms: Vec::new(),
            fixtures: Vec::new(),
            next_building_id: 1,
            next_room_id: 1,
            next_fixture_id: 1,
        }
    }

    fn fill(&mut self, kind: TileKind) {
        self.grid.tiles.clear();
        for y in 0..self.grid.height {
            for x in 0..self.grid.width {
                self.grid.tiles.push(TileSpec {
                    coord: TileCoord { x, y },
                    kind,
                    building_id: None,
                    room_id: None,
                });
            }
        }
    }

    fn carve_road_rect(&mut self, rect: Rect) {
        for coord in rect.footprint() {
            self.set_tile(coord, TileKind::Road, None, None);
        }
    }

    fn carve_field_rect(&mut self, rect: Rect) {
        for coord in rect.footprint() {
            self.set_tile(coord, TileKind::Field, None, None);
        }
    }

    fn carve_terrain_rect(&mut self, rect: Rect, kind: TileKind) {
        for coord in rect.footprint() {
            self.set_tile(coord, kind, None, None);
        }
    }

    fn carve_road_line(&mut self, start: TileCoord, end: TileCoord) {
        if start.x == end.x {
            let min_y = start.y.min(end.y);
            let max_y = start.y.max(end.y);
            for y in min_y..=max_y {
                self.set_tile(TileCoord { x: start.x, y }, TileKind::Road, None, None);
            }
        } else if start.y == end.y {
            let min_x = start.x.min(end.x);
            let max_x = start.x.max(end.x);
            for x in min_x..=max_x {
                self.set_tile(TileCoord { x, y: start.y }, TileKind::Road, None, None);
            }
        } else {
            // Manhattan path for diagonal roads
            let mid_x = start.x;
            let min_y = start.y.min(end.y);
            let max_y = start.y.max(end.y);
            for y in min_y..=max_y {
                self.set_tile(TileCoord { x: mid_x, y }, TileKind::Road, None, None);
            }
            let mid_y = end.y;
            let min_x = start.x.min(end.x);
            let max_x = start.x.max(end.x);
            for x in min_x..=max_x {
                self.set_tile(TileCoord { x, y: mid_y }, TileKind::Road, None, None);
            }
        }
    }

    fn add_building(
        &mut self,
        name: &str,
        kind: LocationKind,
        rect: Rect,
        entrance: TileCoord,
        room_name: &str,
        room_kind: &str,
        fixtures: Vec<FixturePlacement>,
    ) -> BuildingId {
        let building_id = self.next_building_id;
        self.next_building_id += 1;
        let room_id = self.next_room_id;
        self.next_room_id += 1;

        for coord in rect.border_tiles() {
            self.set_tile(coord, TileKind::Wall, Some(building_id), Some(room_id));
        }
        for coord in rect.interior_tiles() {
            self.set_tile(coord, TileKind::Floor, Some(building_id), Some(room_id));
        }
        self.set_tile(entrance, TileKind::Door, Some(building_id), Some(room_id));

        self.rooms.push(RoomSpec {
            id: room_id,
            building_id,
            name: room_name.to_string(),
            kind: room_kind.to_string(),
            tiles: rect.interior_tiles(),
        });

        self.buildings.push(BuildingSpec {
            id: building_id,
            name: name.to_string(),
            kind,
            entrance,
            room_ids: vec![room_id],
            footprint: rect.footprint(),
        });

        for placement in fixtures {
            self.fixtures.push(FixtureSpec {
                id: self.next_fixture_id,
                building_id: Some(building_id),
                room_id: Some(room_id),
                kind: placement.kind,
                coord: placement.coord,
                name: placement.name.to_string(),
                blocks_movement: placement.blocks_movement,
                stock: placement.stock,
            });
            self.next_fixture_id += 1;
        }

        building_id
    }

    fn add_outdoor_fixture(
        &mut self,
        building_id: Option<BuildingId>,
        room_id: Option<RoomId>,
        kind: FixtureKind,
        coord: TileCoord,
        name: &str,
        blocks_movement: bool,
        stock: Vec<ResourceStack>,
    ) {
        self.fixtures.push(FixtureSpec {
            id: self.next_fixture_id,
            building_id,
            room_id,
            kind,
            coord,
            name: name.to_string(),
            blocks_movement,
            stock,
        });
        self.next_fixture_id += 1;
    }

    fn set_tile(
        &mut self,
        coord: TileCoord,
        kind: TileKind,
        building_id: Option<BuildingId>,
        room_id: Option<RoomId>,
    ) {
        if coord.x >= 0 && coord.x < self.grid.width && coord.y >= 0 && coord.y < self.grid.height {
            let index = (coord.y * self.grid.width + coord.x) as usize;
            if let Some(tile) = self.grid.tiles.get_mut(index) {
                if kind == TileKind::Road && tile.kind == TileKind::Door {
                    return;
                }
                tile.kind = kind;
                tile.building_id = building_id;
                tile.room_id = room_id;
            }
        }
    }

    fn finish(self) -> SpatialSnapshot {
        SpatialSnapshot {
            grid: self.grid,
            buildings: self.buildings,
            rooms: self.rooms,
            fixtures: self.fixtures,
        }
    }
}

const NAMES_POOL: &[&str] = &[
    "Alda",
    "Breno",
    "Celia",
    "Dario",
    "Elina",
    "Faro",
    "Gisa",
    "Helmo",
    "Iria",
    "Joran",
    "Kelda",
    "Lute",
    "Martim",
    "Nuno",
    "Olga",
    "Pedro",
    "Quelia",
    "Rui",
    "Sancha",
    "Tomas",
    "Ugo",
    "Vasco",
    "Ximena",
    "Zaria",
    "Afonso",
    "Beatriz",
    "Constanca",
    "Duarte",
    "Estevao",
    "Filipa",
    "Goncalo",
    "Henrique",
    "Ines",
    "Joao",
    "Leonor",
    "Manuel",
    "Mafalda",
    "Orlandina",
    "Pinto",
    "Rodrigo",
    "Sancho",
    "Teresa",
    "Vicente",
    "Vera",
];

const TRAITS_POOL: &[&[&str]] = &[
    &["observador", "teimoso"],
    &["generoso", "cauteloso"],
    &["trabalhador", "orgulhoso"],
    &["curioso", "desconfiado"],
    &["impulsivo", "ambicioso"],
    &["astuto", "ressentido"],
    &["covarde", "oportunista"],
    &["violento", "leal"],
];

const VALUES_POOL: &[&[&str]] = &[
    &["honra", "sobrevivencia"],
    &["familia", "comunidade"],
    &["riqueza", "justica"],
    &["poder", "vinganca"],
    &["liberdade", "prazer"],
];

const FEARS_POOL: &[&[&str]] = &[
    &["escassez", "humilhacao"],
    &["solidao", "doenca"],
    &["violencia", "fracasso"],
    &["traicao", "irrelevancia"],
    &["aprisionamento", "impotencia"],
];

const DESIRES_POOL: &[&[&str]] = &[
    &["seguranca para a familia"],
    &["acumular riqueza"],
    &["conquistar respeito"],
    &["viver sem ser controlado"],
    &["vingar injusticas passadas"],
];

const TOLERANCE_POOL: &[&str] = &[
    "mente por protecao",
    "rouba se com fome extrema",
    "aceita violencia quando provocado",
    "tolera suborno por necessidade",
    "ignora crimes de aliados",
];

const STYLE_POOL: &[&str] = &[
    "prudente",
    "agressivo",
    "manipulador",
    "submisso",
    "confrontador",
    "sedutor",
    "isolado",
];

fn initial_institutional_perception(role_id: &str, values: &[&str]) -> InstitutionalPerception {
    let mut perception = InstitutionalPerception::default();
    if role_id == "lider_local" {
        perception.leader_legitimacy += 30;
        perception.justice_legitimacy += 15;
        perception.tax_legitimacy += 10;
        perception.guard_trust += 20;
        perception.war_support += 8;
        perception.perceived_fairness += 10;
    } else if role_id == "guarda" {
        perception.leader_legitimacy += 18;
        perception.justice_legitimacy += 20;
        perception.guard_trust += 25;
        perception.war_support += 12;
        perception.fear_of_authority += 6;
    }
    if values
        .iter()
        .any(|value| matches!(*value, "honra" | "ordem" | "justica"))
    {
        perception.leader_legitimacy += 8;
        perception.justice_legitimacy += 8;
        perception.guard_trust += 5;
    }
    if values
        .iter()
        .any(|value| matches!(*value, "sobrevivencia" | "familia"))
    {
        perception.tax_legitimacy -= 4;
        perception.rationing_legitimacy += 4;
    }
    perception.notes.push(format!(
        "percepcao institucional inicial baseada no papel {}",
        role_id
    ));
    perception.clamp_all();
    perception
}

fn merge_stack(stacks: &mut Vec<ResourceStack>, stack: ResourceStack) {
    if let Some(existing) = stacks
        .iter_mut()
        .find(|existing| existing.resource_id == stack.resource_id)
    {
        existing.amount += stack.amount;
    } else {
        stacks.push(stack);
    }
}

pub fn generate_world(config: SimulationConfig) -> Result<SimulationSnapshot, String> {
    if config.grid_width < 100 || config.grid_height < 60 {
        return Err("As dimensoes do grid devem ser de pelo menos 100x60".to_string());
    }
    Ok(generate_procedural_world(config))
}

fn generate_procedural_world(config: SimulationConfig) -> SimulationSnapshot {
    let catalog = default_economy_catalog();
    validate_catalog(&catalog).expect("default economy catalog should be valid");

    let mut builder = SpatialBuilder::new(config.grid_width, config.grid_height);
    builder.fill(TileKind::Grass);

    // 1. Spacing and centers of the villages
    let centers = vec![
        TileCoord { x: 75, y: 22 },
        TileCoord { x: 35, y: 72 },
        TileCoord { x: 115, y: 72 },
    ];
    let num_v = config.num_villages.max(1).min(3);
    let active_centers = centers[..num_v].to_vec();

    // 2. Build diagonal and straight connecting highways between village centers (Manhattan)
    if num_v >= 2 {
        builder.carve_road_line(active_centers[0], active_centers[1]);
    }
    if num_v >= 3 {
        builder.carve_road_line(active_centers[0], active_centers[2]);
        builder.carve_road_line(active_centers[1], active_centers[2]);
    }

    let village_names_pool = vec!["Santa Bruma", "Vale Verde", "Pedra Ruiva"];

    let mut agents = Vec::new();
    let mut home_members = HashMap::<BuildingId, Vec<u64>>::new();
    let mut role_households = HashMap::<String, Vec<BuildingId>>::new();

    // Generate each village's buildings and structures
    for (v, center) in active_centers.iter().enumerate() {
        let cx = center.x;
        let cy = center.y;
        let v_name = village_names_pool[v % village_names_pool.len()];

        // Draw internal village road system
        builder.carve_road_rect(Rect {
            x1: cx - 22,
            y1: cy,
            x2: cx + 22,
            y2: cy,
        });
        builder.carve_road_line(
            TileCoord { x: cx, y: cy - 16 },
            TileCoord { x: cx, y: cy + 17 },
        );

        // Add 4 Homes
        let h1_id = builder.add_building(
            &format!("Casa I de {}", v_name),
            LocationKind::Home,
            Rect {
                x1: cx - 20,
                y1: cy - 15,
                x2: cx - 14,
                y2: cy - 10,
            },
            TileCoord {
                x: cx - 17,
                y: cy - 10,
            },
            "Sala Comum",
            "casa",
            vec![
                fixture(FixtureKind::Bed, cx - 19, cy - 14, "Cama 1", true, vec![]),
                fixture(FixtureKind::Bed, cx - 17, cy - 14, "Cama 2", true, vec![]),
                fixture(FixtureKind::Bed, cx - 15, cy - 14, "Cama 3", true, vec![]),
                fixture(
                    FixtureKind::Table,
                    cx - 17,
                    cy - 12,
                    "Mesa da Casa",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Storage,
                    cx - 15,
                    cy - 12,
                    "Armario da Casa",
                    true,
                    vec![ResourceStack {
                        resource_id: ResourceKind::Pao.id().to_string(),
                        amount: 6,
                    }],
                ),
            ],
        );

        let h2_id = builder.add_building(
            &format!("Casa II de {}", v_name),
            LocationKind::Home,
            Rect {
                x1: cx - 10,
                y1: cy - 15,
                x2: cx - 4,
                y2: cy - 10,
            },
            TileCoord {
                x: cx - 7,
                y: cy - 10,
            },
            "Sala Comum",
            "casa",
            vec![
                fixture(FixtureKind::Bed, cx - 9, cy - 14, "Cama 4", true, vec![]),
                fixture(FixtureKind::Bed, cx - 7, cy - 14, "Cama 5", true, vec![]),
                fixture(FixtureKind::Bed, cx - 5, cy - 14, "Cama 6", true, vec![]),
                fixture(
                    FixtureKind::Table,
                    cx - 7,
                    cy - 12,
                    "Mesa da Casa",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Storage,
                    cx - 5,
                    cy - 12,
                    "Armario da Casa",
                    true,
                    vec![ResourceStack {
                        resource_id: ResourceKind::Pao.id().to_string(),
                        amount: 6,
                    }],
                ),
            ],
        );

        let h3_id = builder.add_building(
            &format!("Casa III de {}", v_name),
            LocationKind::Home,
            Rect {
                x1: cx + 4,
                y1: cy - 15,
                x2: cx + 10,
                y2: cy - 10,
            },
            TileCoord {
                x: cx + 7,
                y: cy - 10,
            },
            "Sala Comum",
            "casa",
            vec![
                fixture(FixtureKind::Bed, cx + 5, cy - 14, "Cama 7", true, vec![]),
                fixture(FixtureKind::Bed, cx + 7, cy - 14, "Cama 8", true, vec![]),
                fixture(FixtureKind::Bed, cx + 9, cy - 14, "Cama 9", true, vec![]),
                fixture(
                    FixtureKind::Table,
                    cx + 7,
                    cy - 12,
                    "Mesa da Casa",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Storage,
                    cx + 9,
                    cy - 12,
                    "Armario da Casa",
                    true,
                    vec![ResourceStack {
                        resource_id: ResourceKind::Pao.id().to_string(),
                        amount: 6,
                    }],
                ),
            ],
        );

        builder.add_building(
            &format!("Casa IV de {}", v_name),
            LocationKind::Home,
            Rect {
                x1: cx + 14,
                y1: cy - 15,
                x2: cx + 20,
                y2: cy - 10,
            },
            TileCoord {
                x: cx + 17,
                y: cy - 10,
            },
            "Sala Comum",
            "casa",
            vec![
                fixture(FixtureKind::Bed, cx + 15, cy - 14, "Cama 10", true, vec![]),
                fixture(FixtureKind::Bed, cx + 17, cy - 14, "Cama 11", true, vec![]),
                fixture(FixtureKind::Bed, cx + 19, cy - 14, "Cama 12", true, vec![]),
                fixture(
                    FixtureKind::Table,
                    cx + 17,
                    cy - 12,
                    "Mesa da Casa",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Storage,
                    cx + 19,
                    cy - 12,
                    "Armario da Casa",
                    true,
                    vec![ResourceStack {
                        resource_id: ResourceKind::Pao.id().to_string(),
                        amount: 6,
                    }],
                ),
            ],
        );

        // Solar do Conselho
        let solar_id = builder.add_building(
            &format!("Solar do Conselho de {}", v_name),
            LocationKind::Manor,
            Rect {
                x1: cx - 20,
                y1: cy - 7,
                x2: cx - 10,
                y2: cy - 1,
            },
            TileCoord {
                x: cx - 15,
                y: cy - 1,
            },
            "Sala do Conselho",
            "manor",
            vec![
                fixture(
                    FixtureKind::Table,
                    cx - 15,
                    cy - 4,
                    "Mesa do Conselho",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Seat,
                    cx - 17,
                    cy - 4,
                    "Assento do Conselho",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Workstation,
                    cx - 13,
                    cy - 4,
                    "Escrivaninha do Lider",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Storage,
                    cx - 12,
                    cy - 2,
                    "Arquivo do Solar",
                    true,
                    vec![ResourceStack {
                        resource_id: ResourceKind::Moedas.id().to_string(),
                        amount: 0,
                    }],
                ),
                fixture(
                    FixtureKind::Bed,
                    cx - 18,
                    cy - 2,
                    "Leito do Lider",
                    true,
                    vec![],
                ),
            ],
        );

        // Posto da Muralha
        let guarda_id = builder.add_building(
            &format!("Posto da Muralha de {}", v_name),
            LocationKind::GuardPost,
            Rect {
                x1: cx + 10,
                y1: cy - 7,
                x2: cx + 16,
                y2: cy - 1,
            },
            TileCoord {
                x: cx + 13,
                y: cy - 1,
            },
            "Sala da Guarda",
            "guarda",
            vec![
                fixture(
                    FixtureKind::Workstation,
                    cx + 12,
                    cy - 4,
                    "Mesa da Ronda",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Storage,
                    cx + 14,
                    cy - 4,
                    "Arca da Guarda",
                    true,
                    vec![ResourceStack {
                        resource_id: ResourceKind::Moedas.id().to_string(),
                        amount: 0,
                    }],
                ),
                fixture(
                    FixtureKind::Bed,
                    cx + 12,
                    cy - 2,
                    "Catre da Guarda",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Table,
                    cx + 14,
                    cy - 2,
                    "Mesa da Guarda",
                    true,
                    vec![],
                ),
            ],
        );

        // Forja de Aço
        let forja_id = builder.add_building(
            &format!("Forja de Aco de {}", v_name),
            LocationKind::Workshop,
            Rect {
                x1: cx - 20,
                y1: cy + 3,
                x2: cx - 12,
                y2: cy + 9,
            },
            TileCoord {
                x: cx - 16,
                y: cy + 3,
            },
            "Sala da Forja",
            "forja",
            vec![
                fixture(
                    FixtureKind::Workstation,
                    cx - 18,
                    cy + 5,
                    "Bigorna",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Storage,
                    cx - 14,
                    cy + 5,
                    "Baú de Ferramentas",
                    true,
                    vec![
                        ResourceStack {
                            resource_id: ResourceKind::Ferramentas.id().to_string(),
                            amount: 4,
                        },
                        ResourceStack {
                            resource_id: ResourceKind::MetalBruto.id().to_string(),
                            amount: 4,
                        },
                        ResourceStack {
                            resource_id: ResourceKind::Lenha.id().to_string(),
                            amount: 4,
                        },
                    ],
                ),
                fixture(
                    FixtureKind::Table,
                    cx - 16,
                    cy + 7,
                    "Mesa da Forja",
                    true,
                    vec![],
                ),
            ],
        );

        // Taverna
        let taverna_id = builder.add_building(
            &format!("Taverna da Chuva de {}", v_name),
            LocationKind::Tavern,
            Rect {
                x1: cx - 10,
                y1: cy + 3,
                x2: cx - 1,
                y2: cy + 9,
            },
            TileCoord {
                x: cx - 5,
                y: cy + 3,
            },
            "Sala da Taverna",
            "taverna",
            vec![
                fixture(
                    FixtureKind::Workstation,
                    cx - 7,
                    cy + 5,
                    "Balcao da Taverna",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Storage,
                    cx - 3,
                    cy + 5,
                    "Barril da Taverna",
                    true,
                    vec![
                        ResourceStack {
                            resource_id: ResourceKind::Caldo.id().to_string(),
                            amount: 12,
                        },
                        ResourceStack {
                            resource_id: ResourceKind::Pao.id().to_string(),
                            amount: 6,
                        },
                        ResourceStack {
                            resource_id: ResourceKind::Lenha.id().to_string(),
                            amount: 7,
                        },
                        ResourceStack {
                            resource_id: ResourceKind::Graos.id().to_string(),
                            amount: 1,
                        },
                    ],
                ),
                fixture(
                    FixtureKind::Table,
                    cx - 5,
                    cy + 7,
                    "Mesa Longa",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Seat,
                    cx - 3,
                    cy + 7,
                    "Banco da Taverna",
                    true,
                    vec![],
                ),
            ],
        );

        // Padaria
        let padaria_id = builder.add_building(
            &format!("Padaria de {}", v_name),
            LocationKind::Bakery,
            Rect {
                x1: cx + 12,
                y1: cy + 3,
                x2: cx + 20,
                y2: cy + 9,
            },
            TileCoord {
                x: cx + 16,
                y: cy + 3,
            },
            "Sala do Forno",
            "padaria",
            vec![
                fixture(
                    FixtureKind::Workstation,
                    cx + 14,
                    cy + 5,
                    "Forno",
                    true,
                    vec![],
                ),
                fixture(
                    FixtureKind::Storage,
                    cx + 18,
                    cy + 5,
                    "Despensa",
                    true,
                    vec![
                        ResourceStack {
                            resource_id: ResourceKind::Pao.id().to_string(),
                            amount: 10,
                        },
                        ResourceStack {
                            resource_id: ResourceKind::Graos.id().to_string(),
                            amount: 12,
                        },
                        ResourceStack {
                            resource_id: ResourceKind::Lenha.id().to_string(),
                            amount: 5,
                        },
                    ],
                ),
                fixture(
                    FixtureKind::Table,
                    cx + 16,
                    cy + 7,
                    "Mesa da Padaria",
                    true,
                    vec![],
                ),
            ],
        );

        // Galpão do Lenhal
        let lenhal_id = builder.add_building(
            &format!("Galpao do Lenhal de {}", v_name),
            LocationKind::Woodlot,
            Rect {
                x1: cx - 20,
                y1: cy + 12,
                x2: cx - 14,
                y2: cy + 17,
            },
            TileCoord {
                x: cx - 17,
                y: cy + 12,
            },
            "Abrigo do Lenhal",
            "lenhal",
            vec![
                fixture(
                    FixtureKind::Storage,
                    cx - 16,
                    cy - 12 + 26, // inside the building
                    "Pilha de Lenha",
                    true,
                    vec![ResourceStack {
                        resource_id: ResourceKind::Lenha.id().to_string(),
                        amount: 6,
                    }],
                ),
                fixture(
                    FixtureKind::Table,
                    cx - 18,
                    cy + 14,
                    "Mesa do Lenhal",
                    true,
                    vec![],
                ),
            ],
        );

        // Celeiro (Farm)
        let celeiro_id = builder.add_building(
            &format!("Celeiro de {}", v_name),
            LocationKind::Farm,
            Rect {
                x1: cx - 4,
                y1: cy + 12,
                x2: cx + 4,
                y2: cy + 17,
            },
            TileCoord { x: cx, y: cy + 12 },
            "Sala do Celeiro",
            "campo",
            vec![
                fixture(
                    FixtureKind::Storage,
                    cx + 2,
                    cy + 14,
                    "Armazem do Celeiro",
                    true,
                    vec![ResourceStack {
                        resource_id: ResourceKind::Ferramentas.id().to_string(),
                        amount: 2,
                    }],
                ),
                fixture(
                    FixtureKind::Table,
                    cx - 2,
                    cy + 14,
                    "Mesa do Celeiro",
                    true,
                    vec![],
                ),
            ],
        );

        // Pedreira
        let pedreira_id = builder.add_building(
            &format!("Barracao da Pedreira de {}", v_name),
            LocationKind::Quarry,
            Rect {
                x1: cx + 14,
                y1: cy + 12,
                x2: cx + 20,
                y2: cy + 17,
            },
            TileCoord {
                x: cx + 17,
                y: cy + 12,
            },
            "Abrigo da Pedreira",
            "pedreira",
            vec![
                fixture(
                    FixtureKind::Storage,
                    cx + 18,
                    cy + 14,
                    "Caixote de Minerio",
                    true,
                    vec![ResourceStack {
                        resource_id: ResourceKind::MetalBruto.id().to_string(),
                        amount: 6,
                    }],
                ),
                fixture(
                    FixtureKind::Table,
                    cx + 16,
                    cy + 14,
                    "Mesa da Pedreira",
                    true,
                    vec![],
                ),
            ],
        );

        // Internal Roads connecting doors to Cy Main Road
        builder.carve_road_line(
            TileCoord {
                x: cx - 17,
                y: cy - 10,
            },
            TileCoord { x: cx - 17, y: cy },
        );
        builder.carve_road_line(
            TileCoord {
                x: cx - 7,
                y: cy - 10,
            },
            TileCoord { x: cx - 7, y: cy },
        );
        builder.carve_road_line(
            TileCoord {
                x: cx + 7,
                y: cy - 10,
            },
            TileCoord { x: cx + 7, y: cy },
        );
        builder.carve_road_line(
            TileCoord {
                x: cx + 17,
                y: cy - 10,
            },
            TileCoord { x: cx + 17, y: cy },
        );
        builder.carve_road_line(
            TileCoord {
                x: cx - 15,
                y: cy - 1,
            },
            TileCoord { x: cx - 15, y: cy },
        );
        builder.carve_road_line(
            TileCoord {
                x: cx + 13,
                y: cy - 1,
            },
            TileCoord { x: cx + 13, y: cy },
        );
        builder.carve_road_line(
            TileCoord {
                x: cx - 16,
                y: cy + 3,
            },
            TileCoord { x: cx - 16, y: cy },
        );
        builder.carve_road_line(
            TileCoord {
                x: cx - 5,
                y: cy + 3,
            },
            TileCoord { x: cx - 5, y: cy },
        );
        builder.carve_road_line(
            TileCoord {
                x: cx + 16,
                y: cy + 3,
            },
            TileCoord { x: cx + 16, y: cy },
        );
        builder.carve_road_line(
            TileCoord {
                x: cx - 17,
                y: cy + 12,
            },
            TileCoord { x: cx - 17, y: cy },
        );
        builder.carve_road_line(
            TileCoord {
                x: cx + 17,
                y: cy + 12,
            },
            TileCoord { x: cx + 17, y: cy },
        );

        // Outdoor Workstations
        builder.add_outdoor_fixture(
            Some(celeiro_id),
            None,
            FixtureKind::Workstation,
            TileCoord {
                x: cx - 2,
                y: cy + 18,
            },
            "Leira de Plantio",
            false,
            vec![],
        );
        builder.add_outdoor_fixture(
            Some(celeiro_id),
            None,
            FixtureKind::Workstation,
            TileCoord {
                x: cx + 2,
                y: cy + 18,
            },
            "Sulco de Plantio",
            false,
            vec![],
        );
        builder.add_outdoor_fixture(
            Some(lenhal_id),
            None,
            FixtureKind::Workstation,
            TileCoord {
                x: cx - 18,
                y: cy + 18,
            },
            "Tronco de Corte",
            false,
            vec![],
        );
        builder.add_outdoor_fixture(
            Some(lenhal_id),
            None,
            FixtureKind::Workstation,
            TileCoord {
                x: cx - 15,
                y: cy + 18,
            },
            "Clareira de Coleta",
            false,
            vec![],
        );
        builder.add_outdoor_fixture(
            Some(pedreira_id),
            None,
            FixtureKind::Workstation,
            TileCoord {
                x: cx + 16,
                y: cy + 18,
            },
            "Face da Pedreira",
            false,
            vec![],
        );
        builder.add_outdoor_fixture(
            Some(pedreira_id),
            None,
            FixtureKind::Workstation,
            TileCoord {
                x: cx + 19,
                y: cy + 18,
            },
            "Veio Exposto",
            false,
            vec![],
        );

        // Natural Resources
        builder.carve_terrain_rect(
            Rect {
                x1: cx - 22,
                y1: cy + 19,
                x2: cx - 12,
                y2: cy + 21,
            },
            TileKind::Forest,
        );
        builder.carve_field_rect(Rect {
            x1: cx - 6,
            y1: cy + 19,
            x2: cx + 6,
            y2: cy + 21,
        });
        builder.carve_terrain_rect(
            Rect {
                x1: cx + 12,
                y1: cy + 19,
                x2: cx + 22,
                y2: cy + 21,
            },
            TileKind::Rock,
        );

        // Generate 7 agents for this village
        let role_assignments = vec![
            (
                "lider_local",
                solar_id,
                h1_id,
                TileCoord {
                    x: cx - 19,
                    y: cy - 14,
                },
            ),
            (
                "taverneiro",
                taverna_id,
                h1_id,
                TileCoord {
                    x: cx - 17,
                    y: cy - 14,
                },
            ),
            (
                "ferreiro",
                forja_id,
                h1_id,
                TileCoord {
                    x: cx - 15,
                    y: cy - 14,
                },
            ),
            (
                "padeiro",
                padaria_id,
                h2_id,
                TileCoord {
                    x: cx - 9,
                    y: cy - 14,
                },
            ),
            (
                "guarda",
                guarda_id,
                h2_id,
                TileCoord {
                    x: cx - 7,
                    y: cy - 14,
                },
            ),
            (
                "campones",
                celeiro_id,
                h2_id,
                TileCoord {
                    x: cx - 5,
                    y: cy - 14,
                },
            ),
            (
                "campones",
                lenhal_id,
                h3_id,
                TileCoord {
                    x: cx + 5,
                    y: cy - 14,
                },
            ),
        ];

        for (idx, (role_id, work_id, home_id, bed)) in role_assignments.into_iter().enumerate() {
            let relative_agent_idx = v * 7 + idx;
            let agent_id = (relative_agent_idx as u64) + 1;
            let name = NAMES_POOL[relative_agent_idx % NAMES_POOL.len()].to_string();

            // Unique deterministic profiles
            let traits = TRAITS_POOL[relative_agent_idx % TRAITS_POOL.len()]
                .iter()
                .map(|s| s.to_string())
                .collect();
            let values = VALUES_POOL[relative_agent_idx % VALUES_POOL.len()]
                .iter()
                .map(|s| s.to_string())
                .collect();
            let fears = FEARS_POOL[relative_agent_idx % FEARS_POOL.len()]
                .iter()
                .map(|s| s.to_string())
                .collect();

            // Set up relations later
            home_members.entry(home_id).or_default().push(agent_id);
            role_households
                .entry(role_id.to_string())
                .or_default()
                .push(home_id);

            agents.push(AgentSnapshot {
                id: agent_id,
                name: name.clone(),
                role_id: role_id.to_string(),
                home_building_id: Some(home_id),
                work_building_id: Some(work_id),
                home_bed: Some(bed),
                profile: AgentProfile {
                    traits,
                    values,
                    fears,
                    long_term_desires: DESIRES_POOL[relative_agent_idx % DESIRES_POOL.len()]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    moral_tolerances: vec![
                        TOLERANCE_POOL[relative_agent_idx % TOLERANCE_POOL.len()].to_string(),
                    ],
                    social_style: STYLE_POOL[relative_agent_idx % STYLE_POOL.len()].to_string(),
                    trauma_traits: Vec::new(),
                },
                state: AgentState {
                    mood: 55 + (idx as i32 * 3) % 15,
                    energy: 65 + (idx as i32 * 2) % 15,
                    health: 100,
                    hunger: 20 + (idx as i32 * 4) % 15,
                    stress: 10 + (idx as i32 * 5) % 15,
                    current_focus: "manter rotina".to_string(),
                    active_goals: vec!["proteger reputacao".to_string()],
                },
                life_status: AgentLifeStatus::Vivo,
                injury: InjuryState::default(),
                institutional_perception: initial_institutional_perception(
                    role_id,
                    VALUES_POOL[relative_agent_idx % VALUES_POOL.len()],
                ),
                psychological_state: PsychologicalState::default(),
                rumor_beliefs: Vec::new(),
                story_beliefs: Vec::new(),
                relations: HashMap::new(), // will fill next
                memories: Vec::new(),
                inventory: Vec::new(),
                position: bed,
                destination: None,
                destination_label: None,
                planned_path: Vec::new(),
                current_building_id: None,
                current_room_id: None,
                active_conversation_id: None,
                conversation_partner_id: None,
                last_social_act: None,
                social_cooldown_until: 0,
                last_intent: None,
                task_queue: Vec::new(),
                last_thought: format!("{} mede o humor da vila antes de agir.", name),
                llm_cooldown_until: 0,
                llm_calls: 0,
                active_economic_task_id: None,
                carrying: Vec::new(),
                carrying_capacity: 4,
                next_reconsideration_tick: 0,
                blocked_ticks: 0,
                last_cognition_trigger: None,
                last_social_opportunity_signature: None,
                last_deliberation_hunger: 25,
                last_deliberation_energy: 65,
                last_deliberation_health: 100,
                last_deliberation_stress: 10,
                trauma_tracker: TraumaTracker::default(),
                age: 20 + ((relative_agent_idx * 7) % 25) as u32,
                parents: Vec::new(),
                children: Vec::new(),
                spouse: None,
                gender: if relative_agent_idx % 2 == 0 {
                    "Masculino".to_string()
                } else {
                    "Feminino".to_string()
                },
            });
        }
    }

    // Truncate to config.max_agents if specified
    if config.max_agents > 0 && agents.len() > config.max_agents {
        agents.truncate(config.max_agents);
    }

    // Build relations between all agents
    let agent_ids: Vec<u64> = agents.iter().map(|a| a.id).collect();
    for i in 0..agents.len() {
        let id = agents[i].id;
        // Determine village of agent id
        let village_i = (id - 1) / 7;
        let mut relations = HashMap::new();
        for &other in &agent_ids {
            if other == id {
                continue;
            }
            let village_other = (other - 1) / 7;
            if village_i == village_other {
                // Same village (slight positive relation)
                relations.insert(
                    other,
                    AgentRelation {
                        trust: 10 + ((id + other) % 10) as i32,
                        friendship: 5 + ((id * other) % 10) as i32,
                        resentment: ((id + other) % 5) as i32,
                        attraction: ((id * other) % 15) as i32,
                        moral_debt: 0,
                        reputation: 5 + ((id as i32 - other as i32).abs() % 10),
                        last_updated_day: 1,
                        notes: Vec::new(),
                    },
                );
            } else {
                // Different villages (xenophobia/neutral relation)
                relations.insert(
                    other,
                    AgentRelation {
                        trust: -5 - ((id + other) % 5) as i32,
                        friendship: 0,
                        resentment: 0,
                        attraction: 0,
                        moral_debt: 0,
                        reputation: 0,
                        last_updated_day: 1,
                        notes: Vec::new(),
                    },
                );
            }
        }
        agents[i].relations = relations;
    }

    let spatial = builder.finish();

    for households in role_households.values_mut() {
        households.sort_unstable();
        households.dedup();
    }

    let home_building_ids: HashSet<BuildingId> = home_members.keys().copied().collect();

    // Setup households
    let mut households = spatial
        .buildings
        .iter()
        .filter(|building| {
            building.kind == LocationKind::Home || home_building_ids.contains(&building.id)
        })
        .map(|building| {
            let pantry = spatial
                .fixtures
                .iter()
                .find(|fixture| {
                    fixture.kind == FixtureKind::Storage && fixture.building_id == Some(building.id)
                })
                .map(|fixture| fixture.stock.clone())
                .unwrap_or_default();
            let member_ids = home_members.get(&building.id).cloned().unwrap_or_default();
            HouseholdEconomy {
                id: building.id,
                name: building.name.clone(),
                member_ids: member_ids.clone(),
                treasury: 30, // 30 starting treasury for stability
                pantry,
                reserved_food: Vec::new(),
                minimum_food_units: (member_ids.len() as i32).max(1) * 3,
                pending_payments: Vec::new(),
                scarcity_pressure: 0,
                food_crisis_level: 0,
                reserved_food_workers: 0,
                last_food_shortage_tick: 0,
                tax_arrears: 0,
                last_tax_paid_day: 0,
                direct_lord_agent_id: None,
                feudal_tribute_due: 0,
                corvee_days_due: 0,
                levy_service_due: 0,
                feudal_arrears: 0,
            }
        })
        .collect::<Vec<_>>();

    // Setup establishments
    let establishments = spatial
        .buildings
        .iter()
        .filter_map(|building| {
            let storage = spatial.fixtures.iter().find(|fixture| {
                fixture.kind == FixtureKind::Storage && fixture.building_id == Some(building.id)
            });
            let establishment_type = catalog
                .establishment_types
                .iter()
                .find(|entry| entry.location_kind == building.kind)?;
            let stock_targets = establishment_type.stock_targets.clone();
            let wage_per_shift = establishment_type.wage_per_shift;
            let public_service = establishment_type.public_service;
            let default_stock = establishment_type.default_stock.clone();

            // Recompute owner policy relatives
            let owner_household_ids = match &establishment_type.owner_policy {
                crate::world_model::OwnerPolicyDef::PrivateByRole { role_id } => {
                    // find households assigned to this role in this village
                    let village_of_building = (building.id - 1) / 12;
                    role_households
                        .get(role_id)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|&h_id| (h_id - 1) / 12 == village_of_building)
                        .collect()
                }
                crate::world_model::OwnerPolicyDef::SharedByRoles { role_ids } => {
                    let village_of_building = (building.id - 1) / 12;
                    let mut owners = Vec::new();
                    for role_id in role_ids {
                        owners.extend(
                            role_households
                                .get(role_id)
                                .cloned()
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|&h_id| (h_id - 1) / 12 == village_of_building),
                        );
                    }
                    owners.sort_unstable();
                    owners.dedup();
                    owners
                }
                crate::world_model::OwnerPolicyDef::Civic => Vec::new(),
            };

            let mut stock = storage
                .map(|fixture| fixture.stock.clone())
                .unwrap_or_default();
            for stack in default_stock {
                merge_stack(&mut stock, stack);
            }

            let posted_prices = catalog
                .resources
                .iter()
                .filter(|resource| {
                    stock_targets
                        .iter()
                        .any(|target| target.resource_id == resource.id)
                })
                .map(|resource| PostedPrice {
                    resource_id: resource.id.clone(),
                    unit_price: resource.base_price,
                })
                .collect::<Vec<_>>();

            Some(EstablishmentEconomy {
                id: building.id,
                building_id: Some(building.id),
                name: building.name.clone(),
                establishment_type_id: establishment_type.id.clone(),
                location_kind: building.kind,
                owner_household_ids,
                storage_fixture_id: storage.map(|fixture| fixture.id),
                cash: if public_service { 0 } else { 30 },
                stock,
                stock_targets,
                posted_prices,
                wage_per_shift,
                tool_wear: 0,
                public_service,
            })
        })
        .collect::<Vec<_>>();

    let village_economy = VillageEconomy {
        public_treasury: 140 * num_v as i32,
        daily_household_tax: 1,
        external_market_coord: TileCoord {
            x: config.grid_width / 2,
            y: config.grid_height / 2,
        },
        base_prices: catalog
            .resources
            .iter()
            .map(|resource| PostedPrice {
                resource_id: resource.id.clone(),
                unit_price: resource.base_price,
            })
            .collect(),
        external_quotes: catalog
            .external_market_rules
            .iter()
            .map(|rule| crate::world_model::ExternalMarketQuote {
                resource_id: rule.resource_id.clone(),
                buy_price: rule.buy_price,
                sell_price: rule.sell_price,
            })
            .collect(),
        scarcity_metrics: Vec::new(),
    };

    let local_polity_id = 1;
    let leader_id = agents
        .iter()
        .find(|agent| agent.role_id == "lider_local")
        .map(|agent| agent.id);
    let polities = vec![Polity {
        id: local_polity_id,
        name: format!("Conselho de {}", config.village_name),
        ruler_agent_id: leader_id,
        capital_territory_id: Some(1),
        treasury: village_economy.public_treasury,
        military_readiness: 10,
    }];

    let mut territories = Vec::new();
    territories.push(Territory {
        id: 1,
        name: "Vila central".to_string(),
        controller_polity_id: local_polity_id,
        claimed_by: vec![local_polity_id],
        building_ids: spatial
            .buildings
            .iter()
            .map(|building| building.id)
            .collect(),
        tile_coords: spatial
            .grid
            .tiles
            .iter()
            .filter(|tile| !matches!(tile.kind, TileKind::Field))
            .map(|tile| tile.coord)
            .collect(),
        stability: 70,
        strategic_value: 30,
        control_pressure: vec![TerritoryControlPressure {
            polity_id: local_polity_id,
            pressure: 70,
        }],
    });

    let territory_specs = [
        ("Campos", LocationKind::Farm, 50),
        ("Lenhal", LocationKind::Woodlot, 35),
        ("Pedreira", LocationKind::Quarry, 35),
        ("Distrito civico", LocationKind::Manor, 45),
    ];
    let mut next_territory_id = 2;
    for (name, kind, strategic_value) in territory_specs {
        let building_ids = spatial
            .buildings
            .iter()
            .filter(|building| building.kind == kind)
            .map(|building| building.id)
            .collect::<Vec<_>>();
        if building_ids.is_empty() {
            continue;
        }
        let tile_coords = spatial
            .grid
            .tiles
            .iter()
            .filter(|tile| {
                tile.building_id
                    .is_some_and(|id| building_ids.contains(&id))
            })
            .map(|tile| tile.coord)
            .collect::<Vec<_>>();
        territories.push(Territory {
            id: next_territory_id,
            name: name.to_string(),
            controller_polity_id: local_polity_id,
            claimed_by: vec![local_polity_id],
            building_ids,
            tile_coords,
            stability: 65,
            strategic_value,
            control_pressure: vec![TerritoryControlPressure {
                polity_id: local_polity_id,
                pressure: 65,
            }],
        });
        next_territory_id += 1;
    }

    let captain_id = agents
        .iter()
        .find(|agent| agent.role_id == "guarda")
        .map(|agent| agent.id);
    let field_vassal_id = agents
        .iter()
        .find(|agent| agent.role_id == "campones")
        .map(|agent| agent.id);
    let civic_steward_id = agents
        .iter()
        .find(|agent| agent.role_id == "taverneiro" || agent.role_id == "ferreiro")
        .map(|agent| agent.id)
        .or(captain_id);

    for household in &mut households {
        household.direct_lord_agent_id = leader_id;
        household.feudal_tribute_due = 1;
        household.corvee_days_due = 1;
        household.levy_service_due = if household.member_ids.len() >= 3 {
            1
        } else {
            0
        };
    }

    let mut next_estate_holding_id = 1;
    let estate_holdings = territories
        .iter()
        .map(|territory| {
            let holder_agent_id = if territory.name == "Campos" {
                field_vassal_id.or(leader_id)
            } else if territory.name == "Distrito civico" {
                civic_steward_id.or(leader_id)
            } else {
                leader_id
            };
            let building_ids = territory.building_ids.clone();
            let establishment_ids = establishments
                .iter()
                .filter(|entry| {
                    entry
                        .building_id
                        .is_some_and(|building_id| building_ids.contains(&building_id))
                })
                .map(|entry| entry.id)
                .collect::<Vec<_>>();
            let holding_id = next_estate_holding_id;
            next_estate_holding_id += 1;
            EstateHolding {
                id: holding_id,
                name: format!("Dominio de {}", territory.name),
                holder_kind: EstateHolderKind::Agent,
                holder_agent_id,
                holder_household_id: holder_agent_id.and_then(|agent_id| {
                    agents
                        .iter()
                        .find(|agent| agent.id == agent_id)
                        .and_then(|agent| agent.home_building_id)
                }),
                holder_polity_id: Some(local_polity_id),
                territory_id: territory.id,
                building_ids,
                establishment_ids,
                annualized_value: territory.strategic_value * 12,
                tribute_share_percent: if territory.name == "Vila central" {
                    20
                } else {
                    12
                },
                labor_obligation_days: if territory.name == "Campos" { 2 } else { 1 },
                military_obligation: if territory.name == "Distrito civico" {
                    4
                } else {
                    2
                },
            }
        })
        .collect::<Vec<_>>();

    let lord_title_id = 1;
    let field_title_id = 2;
    let civic_title_id = 3;
    let guard_title_id = 4;
    let feudal_titles = vec![
        FeudalTitle {
            id: lord_title_id,
            name: format!("Senhor de {}", config.village_name),
            rank: FeudalRank::Senhor,
            holder_agent_id: leader_id,
            polity_id: Some(local_polity_id),
            territory_id: Some(1),
            holding_id: estate_holdings
                .iter()
                .find(|holding| holding.territory_id == 1)
                .map(|holding| holding.id),
            suzerain_title_id: None,
            succession_rule: SuccessionRule::HerdeiroDireto,
            legitimacy: 65,
            precedence: 100,
            active: true,
        },
        FeudalTitle {
            id: field_title_id,
            name: "Vassalo dos Campos".to_string(),
            rank: FeudalRank::Cavaleiro,
            holder_agent_id: field_vassal_id,
            polity_id: Some(local_polity_id),
            territory_id: territories
                .iter()
                .find(|t| t.name == "Campos")
                .map(|t| t.id),
            holding_id: estate_holdings
                .iter()
                .find(|holding| {
                    territories
                        .iter()
                        .find(|t| t.id == holding.territory_id)
                        .is_some_and(|t| t.name == "Campos")
                })
                .map(|holding| holding.id),
            suzerain_title_id: Some(lord_title_id),
            succession_rule: SuccessionRule::HerdeiroDireto,
            legitimacy: 52,
            precedence: 70,
            active: true,
        },
        FeudalTitle {
            id: civic_title_id,
            name: "Administrador do Solar".to_string(),
            rank: FeudalRank::Oficial,
            holder_agent_id: civic_steward_id,
            polity_id: Some(local_polity_id),
            territory_id: territories
                .iter()
                .find(|t| t.name == "Distrito civico")
                .map(|t| t.id),
            holding_id: estate_holdings
                .iter()
                .find(|holding| {
                    territories
                        .iter()
                        .find(|t| t.id == holding.territory_id)
                        .is_some_and(|t| t.name == "Distrito civico")
                })
                .map(|holding| holding.id),
            suzerain_title_id: Some(lord_title_id),
            succession_rule: SuccessionRule::NomeacaoDoSuserano,
            legitimacy: 58,
            precedence: 60,
            active: true,
        },
        FeudalTitle {
            id: guard_title_id,
            name: "Capitao da Guarda".to_string(),
            rank: FeudalRank::Oficial,
            holder_agent_id: captain_id,
            polity_id: Some(local_polity_id),
            territory_id: Some(1),
            holding_id: None,
            suzerain_title_id: Some(lord_title_id),
            succession_rule: SuccessionRule::NomeacaoDoSuserano,
            legitimacy: 60,
            precedence: 65,
            active: true,
        },
    ];

    let feudal_contracts = vec![
        field_vassal_id.map(|vassal_id| FeudalContract {
            id: 1,
            suzerain_agent_id: leader_id.unwrap_or(vassal_id),
            vassal_agent_id: vassal_id,
            territory_id: territories
                .iter()
                .find(|t| t.name == "Campos")
                .map(|t| t.id),
            holding_id: estate_holdings
                .iter()
                .find(|holding| {
                    territories
                        .iter()
                        .find(|t| t.id == holding.territory_id)
                        .is_some_and(|t| t.name == "Campos")
                })
                .map(|holding| holding.id),
            tribute_due_per_day: 2,
            levy_duty: 2,
            judicial_aid_duty: 0,
            maintenance_duty: 1,
            loyalty: 55,
            coercion: 35,
            perceived_legitimacy: 50,
            status: FeudalContractStatus::Active,
            last_updated_day: 1,
        }),
        captain_id.map(|vassal_id| FeudalContract {
            id: 2,
            suzerain_agent_id: leader_id.unwrap_or(vassal_id),
            vassal_agent_id: vassal_id,
            territory_id: Some(1),
            holding_id: None,
            tribute_due_per_day: 0,
            levy_duty: 3,
            judicial_aid_duty: 3,
            maintenance_duty: 1,
            loyalty: 62,
            coercion: 45,
            perceived_legitimacy: 58,
            status: FeudalContractStatus::Active,
            last_updated_day: 1,
        }),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    let authority_offices = vec![
        AuthorityOffice {
            id: 1,
            kind: AuthorityOfficeKind::CapitaoDaGuarda,
            name: "Capitania da Guarda".to_string(),
            granter_agent_id: leader_id,
            holder_agent_id: captain_id,
            territory_id: Some(1),
            title_id: Some(guard_title_id),
            active: true,
            authority_score: 65,
        },
        AuthorityOffice {
            id: 2,
            kind: AuthorityOfficeKind::AdministradorDoSolar,
            name: "Intendencia do Solar".to_string(),
            granter_agent_id: leader_id,
            holder_agent_id: civic_steward_id,
            territory_id: territories
                .iter()
                .find(|t| t.name == "Distrito civico")
                .map(|t| t.id),
            title_id: Some(civic_title_id),
            active: true,
            authority_score: 54,
        },
    ];

    let power_centers = feudal_titles
        .iter()
        .enumerate()
        .map(|(index, title)| {
            let holding_value = title
                .holding_id
                .and_then(|holding_id| {
                    estate_holdings
                        .iter()
                        .find(|holding| holding.id == holding_id)
                        .map(|holding| holding.annualized_value / 12)
                })
                .unwrap_or(10);
            PowerCenter {
                id: (index as u64) + 1,
                territory_id: title.territory_id,
                title_id: Some(title.id),
                agent_id: title.holder_agent_id,
                formal_authority: title.precedence,
                material_power: holding_value,
                coercive_power: if title.id == guard_title_id { 65 } else { 25 },
                legitimacy: title.legitimacy,
                stability: 60,
                summary: format!(
                    "{} concentra autoridade formal {} e base material {}.",
                    title.name, title.precedence, holding_value
                ),
            }
        })
        .collect::<Vec<_>>();

    SimulationSnapshot {
        schema_version: 20,
        catalog_version: 1,
        village_name: config.village_name,
        day: 1,
        tick_of_day: 0,
        total_ticks: 0,
        ticks_per_day: config.ticks_per_day,
        next_memory_id: 10_000,
        next_conversation_id: 1,
        next_economic_task_id: 1,
        next_construction_project_id: 1,
        next_combat_id: 1,
        next_crime_case_id: 1,
        next_political_faction_id: 1,
        next_political_issue_id: 1,
        next_policy_act_id: 1,
        next_territory_id,
        next_polity_id: 2,
        next_foreign_relation_id: 1,
        next_war_id: 1,
        next_military_demand_id: 1,
        next_insurrection_id: 1,
        next_cultural_story_id: 1,
        next_scheduled_meeting_id: 1,
        next_feudal_title_id: 5,
        next_feudal_contract_id: 3,
        next_estate_holding_id,
        next_succession_crisis_id: 1,
        next_power_center_id: power_centers.len() as u64 + 1,
        next_authority_office_id: authority_offices.len() as u64 + 1,
        agents,
        conversations: Vec::new(),
        scheduled_meetings: Vec::new(),
        combats: Vec::new(),
        crime_cases: Vec::new(),
        political_factions: Vec::new(),
        political_issues: Vec::new(),
        policy_acts: Vec::new(),
        territories,
        polities,
        foreign_relations: Vec::new(),
        wars: Vec::new(),
        military_demands: Vec::new(),
        insurrections: Vec::new(),
        feudal_titles,
        feudal_contracts,
        estate_holdings,
        succession_crises: Vec::new(),
        power_centers,
        authority_offices,
        political_pressures: Vec::new(),
        local_norms: LocalNorms::default(),
        households,
        establishments,
        village_economy,
        economic_tasks: Vec::new(),
        construction_projects: Vec::new(),
        spatial,
        events: Vec::new(),
        crops: std::collections::HashMap::new(),
        secrets: Vec::new(),
        caravans: Vec::new(),
        promises: Vec::new(),
        policy_favors: Vec::new(),
        rumors: Vec::new(),
        cultural_stories: Vec::new(),
        story_versions: Vec::new(),
        cultural_traditions: Vec::new(),
        active_escrows: Vec::new(),
    }
}
