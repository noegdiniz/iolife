use crate::economy_catalog::{default_economy_catalog, validate_catalog};
use crate::sim_core::SimulationConfig;
use crate::world_history::{
    HistoricalEventKind, HistoricalHousehold, HistoricalPerson, HistoricalSettlement,
    HistoricalWorldState, simulate_world_history,
};
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

#[derive(Clone)]
struct RoleLayoutSlot {
    role_id: &'static str,
    work_building_id: BuildingId,
    home_slot_index: usize,
}

#[derive(Clone)]
struct VillageLayout {
    index: usize,
    name: String,
    home_building_ids: [BuildingId; 3],
    role_slots: Vec<RoleLayoutSlot>,
    manor_id: BuildingId,
    guard_post_id: BuildingId,
    workshop_id: BuildingId,
    bakery_id: BuildingId,
    tavern_id: BuildingId,
    farm_id: BuildingId,
    woodlot_id: BuildingId,
    quarry_id: BuildingId,
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

pub fn generate_world(config: SimulationConfig) -> Result<SimulationSnapshot, String> {
    if config.grid_width < 100 || config.grid_height < 60 {
        return Err("As dimensoes do grid devem ser de pelo menos 100x60".to_string());
    }
    let catalog = default_economy_catalog();
    validate_catalog(&catalog).map_err(|error| error.to_string())?;
    Ok(generate_procedural_world(config, &catalog))
}

fn generate_procedural_world(
    config: SimulationConfig,
    catalog: &EconomyCatalog,
) -> SimulationSnapshot {
    let mut builder = SpatialBuilder::new(config.grid_width, config.grid_height);
    builder.fill(TileKind::Grass);
    let history = simulate_world_history(&config, catalog);

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

    let mut village_layouts = Vec::new();

    // Generate each village's buildings and structures
    for (v, center) in active_centers.iter().enumerate() {
        let cx = center.x;
        let cy = center.y;
        let v_name = history
            .settlements
            .get(v)
            .map(|settlement| settlement.name.as_str())
            .unwrap_or(&config.village_name);

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

        village_layouts.push(VillageLayout {
            index: v,
            name: v_name.to_string(),
            home_building_ids: [h1_id, h2_id, h3_id],
            role_slots: vec![
                RoleLayoutSlot {
                    role_id: "lider_local",
                    work_building_id: solar_id,
                    home_slot_index: 0,
                },
                RoleLayoutSlot {
                    role_id: "taverneiro",
                    work_building_id: taverna_id,
                    home_slot_index: 0,
                },
                RoleLayoutSlot {
                    role_id: "ferreiro",
                    work_building_id: forja_id,
                    home_slot_index: 0,
                },
                RoleLayoutSlot {
                    role_id: "padeiro",
                    work_building_id: padaria_id,
                    home_slot_index: 1,
                },
                RoleLayoutSlot {
                    role_id: "guarda",
                    work_building_id: guarda_id,
                    home_slot_index: 1,
                },
                RoleLayoutSlot {
                    role_id: "campones",
                    work_building_id: celeiro_id,
                    home_slot_index: 1,
                },
                RoleLayoutSlot {
                    role_id: "campones",
                    work_building_id: lenhal_id,
                    home_slot_index: 2,
                },
            ],
            manor_id: solar_id,
            guard_post_id: guarda_id,
            workshop_id: forja_id,
            bakery_id: padaria_id,
            tavern_id: taverna_id,
            farm_id: celeiro_id,
            woodlot_id: lenhal_id,
            quarry_id: pedreira_id,
        });
    }

    let spatial = builder.finish();
    let mut materialized =
        materialize_history(&history, &spatial, &village_layouts, &config, catalog);

    if materialized.feudal_contracts.is_empty() && materialized.agents.len() >= 2 {
        let suzerain_agent_id = materialized.agents[0].id;
        if let Some(vassal_agent_id) = materialized
            .agents
            .iter()
            .find(|agent| agent.id != suzerain_agent_id)
            .map(|agent| agent.id)
        {
            materialized.feudal_contracts.push(FeudalContract {
                id: materialized.next_feudal_contract_id,
                suzerain_agent_id,
                vassal_agent_id,
                territory_id: materialized
                    .territories
                    .first()
                    .map(|territory| territory.id),
                holding_id: materialized
                    .estate_holdings
                    .first()
                    .map(|holding| holding.id),
                tribute_due_per_day: 2,
                levy_duty: 1,
                judicial_aid_duty: 1,
                maintenance_duty: 1,
                loyalty: 42,
                coercion: 25,
                perceived_legitimacy: 35,
                status: FeudalContractStatus::Active,
                last_updated_day: 1,
            });
            materialized.next_feudal_contract_id += 1;
        }
    }

    SimulationSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        catalog_version: catalog.version,
        village_name: config.village_name,
        world_history_years_simulated: history.years_simulated,
        world_foundation_year: history.foundation_year,
        historical_summary: Some(history.summary.clone()),
        day: 1,
        tick_of_day: 0,
        total_ticks: 0,
        ticks_per_day: config.ticks_per_day,
        next_memory_id: 10_000,
        next_conversation_id: 1,
        next_economic_task_id: 1,
        next_construction_project_id: materialized.next_construction_project_id,
        next_combat_id: 1,
        next_crime_case_id: materialized.next_crime_case_id,
        next_political_faction_id: materialized.next_political_faction_id,
        next_political_issue_id: materialized.next_political_issue_id,
        next_policy_act_id: materialized.next_policy_act_id,
        next_territory_id: materialized.next_territory_id,
        next_polity_id: materialized.next_polity_id,
        next_foreign_relation_id: materialized.next_foreign_relation_id,
        next_war_id: materialized.next_war_id,
        next_military_demand_id: materialized.next_military_demand_id,
        next_insurrection_id: materialized.next_insurrection_id,
        next_cultural_story_id: materialized.next_cultural_story_id,
        next_scheduled_meeting_id: 1,
        next_social_contract_id: 1,
        next_feudal_title_id: materialized.next_feudal_title_id,
        next_feudal_contract_id: materialized.next_feudal_contract_id,
        next_estate_holding_id: materialized.next_estate_holding_id,
        next_succession_crisis_id: materialized.next_succession_crisis_id,
        next_power_center_id: materialized.next_power_center_id,
        next_authority_office_id: materialized.next_authority_office_id,
        next_item_instance_id: materialized.next_item_instance_id,
        agents: materialized.agents,
        item_instances: materialized.item_instances,
        conversations: Vec::new(),
        scheduled_meetings: Vec::new(),
        social_contracts: Vec::new(),
        combats: Vec::new(),
        crime_cases: materialized.crime_cases,
        political_factions: materialized.political_factions,
        political_issues: materialized.political_issues,
        policy_acts: materialized.policy_acts,
        territories: materialized.territories,
        polities: materialized.polities,
        foreign_relations: materialized.foreign_relations,
        wars: materialized.wars,
        military_demands: materialized.military_demands,
        insurrections: materialized.insurrections,
        feudal_titles: materialized.feudal_titles,
        feudal_contracts: materialized.feudal_contracts,
        estate_holdings: materialized.estate_holdings,
        succession_crises: materialized.succession_crises,
        power_centers: materialized.power_centers,
        authority_offices: materialized.authority_offices,
        political_pressures: materialized.political_pressures,
        local_norms: materialized.local_norms,
        households: materialized.households,
        establishments: materialized.establishments,
        village_economy: materialized.village_economy,
        economic_tasks: Vec::new(),
        construction_projects: materialized.construction_projects,
        spatial,
        events: materialized.events,
        crops: std::collections::HashMap::new(),
        secrets: Vec::new(),
        caravans: Vec::new(),
        promises: Vec::new(),
        policy_favors: Vec::new(),
        rumors: Vec::new(),
        cultural_stories: materialized.cultural_stories,
        story_versions: materialized.story_versions,
        cultural_traditions: materialized.cultural_traditions,
        active_escrows: Vec::new(),
        next_creature_id: 1,
        creatures: Vec::new(),
        next_hunting_quest_id: 1,
        hunting_quests: Vec::new(),
    }
}

struct MaterializedBootstrap {
    agents: Vec<AgentSnapshot>,
    item_instances: Vec<ItemInstance>,
    households: Vec<HouseholdEconomy>,
    establishments: Vec<EstablishmentEconomy>,
    village_economy: VillageEconomy,
    territories: Vec<Territory>,
    polities: Vec<Polity>,
    foreign_relations: Vec<ForeignRelation>,
    wars: Vec<WarState>,
    crime_cases: Vec<CrimeCase>,
    political_factions: Vec<PoliticalFaction>,
    political_issues: Vec<PoliticalIssue>,
    military_demands: Vec<MilitaryDemand>,
    insurrections: Vec<InsurrectionState>,
    policy_acts: Vec<PolicyAct>,
    political_pressures: Vec<PoliticalPressure>,
    feudal_titles: Vec<FeudalTitle>,
    feudal_contracts: Vec<FeudalContract>,
    estate_holdings: Vec<EstateHolding>,
    succession_crises: Vec<SuccessionCrisis>,
    power_centers: Vec<PowerCenter>,
    authority_offices: Vec<AuthorityOffice>,
    cultural_stories: Vec<CulturalStory>,
    story_versions: Vec<StoryVersion>,
    cultural_traditions: Vec<CulturalTradition>,
    construction_projects: Vec<ConstructionProject>,
    events: Vec<WorldEvent>,
    local_norms: LocalNorms,
    next_construction_project_id: u64,
    next_crime_case_id: CrimeCaseId,
    next_political_faction_id: PoliticalFactionId,
    next_political_issue_id: PoliticalIssueId,
    next_policy_act_id: PolicyActId,
    next_territory_id: TerritoryId,
    next_polity_id: PolityId,
    next_foreign_relation_id: ForeignRelationId,
    next_war_id: WarId,
    next_military_demand_id: MilitaryDemandId,
    next_insurrection_id: InsurrectionId,
    next_cultural_story_id: CulturalStoryId,
    next_feudal_title_id: FeudalTitleId,
    next_feudal_contract_id: FeudalContractId,
    next_estate_holding_id: EstateHoldingId,
    next_succession_crisis_id: SuccessionCrisisId,
    next_power_center_id: PowerCenterId,
    next_authority_office_id: AuthorityOfficeId,
    next_item_instance_id: ItemInstanceId,
}

impl Default for MaterializedBootstrap {
    fn default() -> Self {
        Self {
            agents: Vec::new(),
            item_instances: Vec::new(),
            households: Vec::new(),
            establishments: Vec::new(),
            village_economy: VillageEconomy {
                public_treasury: 0,
                daily_household_tax: 0,
                inter_village_trade_coord: TileCoord::default(),
                base_prices: Vec::new(),
                scarcity_metrics: Vec::new(),
            },
            territories: Vec::new(),
            polities: Vec::new(),
            foreign_relations: Vec::new(),
            wars: Vec::new(),
            crime_cases: Vec::new(),
            political_factions: Vec::new(),
            political_issues: Vec::new(),
            military_demands: Vec::new(),
            insurrections: Vec::new(),
            policy_acts: Vec::new(),
            political_pressures: Vec::new(),
            feudal_titles: Vec::new(),
            feudal_contracts: Vec::new(),
            estate_holdings: Vec::new(),
            succession_crises: Vec::new(),
            power_centers: Vec::new(),
            authority_offices: Vec::new(),
            cultural_stories: Vec::new(),
            story_versions: Vec::new(),
            cultural_traditions: Vec::new(),
            construction_projects: Vec::new(),
            events: Vec::new(),
            local_norms: LocalNorms::default(),
            next_construction_project_id: 1,
            next_crime_case_id: 1,
            next_political_faction_id: 1,
            next_political_issue_id: 1,
            next_policy_act_id: 1,
            next_territory_id: 1,
            next_polity_id: 1,
            next_foreign_relation_id: 1,
            next_war_id: 1,
            next_military_demand_id: 1,
            next_insurrection_id: 1,
            next_cultural_story_id: 1,
            next_feudal_title_id: 1,
            next_feudal_contract_id: 1,
            next_estate_holding_id: 1,
            next_succession_crisis_id: 1,
            next_power_center_id: 1,
            next_authority_office_id: 1,
            next_item_instance_id: 1,
        }
    }
}

#[derive(Clone)]
struct SelectedAgentSeed {
    new_id: u64,
    settlement_index: usize,
    historical_person_id: u64,
    role_id: String,
    work_building_id: BuildingId,
    preferred_home_slot_index: usize,
    home_building_id: Option<BuildingId>,
    home_bed: Option<TileCoord>,
}

fn materialize_history(
    history: &HistoricalWorldState,
    spatial: &SpatialSnapshot,
    village_layouts: &[VillageLayout],
    config: &SimulationConfig,
    catalog: &EconomyCatalog,
) -> MaterializedBootstrap {
    let mut out = MaterializedBootstrap {
        local_norms: history
            .settlements
            .first()
            .map(|settlement| settlement.local_norms.clone())
            .unwrap_or_default(),
        next_policy_act_id: 1,
        next_territory_id: 1,
        next_polity_id: 1,
        next_foreign_relation_id: 1,
        next_war_id: 1,
        next_cultural_story_id: 1,
        next_feudal_title_id: 1,
        next_feudal_contract_id: 1,
        next_estate_holding_id: 1,
        next_succession_crisis_id: 1,
        next_power_center_id: 1,
        next_authority_office_id: 1,
        ..MaterializedBootstrap::default()
    };
    let current_year = history.foundation_year + history.years_simulated as i32;
    let mut selected_agent_seeds = Vec::new();
    let mut selected_households: HashMap<(usize, u64), BuildingId> = HashMap::new();
    let mut settlement_primary_households: HashMap<usize, Vec<u64>> = HashMap::new();
    let mut next_agent_id = 1_u64;

    for layout in village_layouts {
        let Some(settlement) = history.settlements.get(layout.index) else {
            continue;
        };
        let mut ranked_households = settlement.households.iter().collect::<Vec<_>>();
        ranked_households
            .sort_by(|a, b| historical_household_score(b).cmp(&historical_household_score(a)));
        let top_households = ranked_households
            .into_iter()
            .take(layout.home_building_ids.len())
            .collect::<Vec<_>>();
        for (home_idx, household) in top_households.iter().enumerate() {
            selected_households.insert(
                (layout.index, household.id),
                layout.home_building_ids[home_idx],
            );
            settlement_primary_households
                .entry(layout.index)
                .or_default()
                .push(household.id);
        }
        let selected_household_ids = top_households.iter().map(|h| h.id).collect::<HashSet<_>>();
        let mut used_people = HashSet::new();
        for slot in &layout.role_slots {
            if next_agent_id > config.max_agents as u64 {
                break;
            }
            let preferred_household_id = settlement_primary_households
                .get(&layout.index)
                .and_then(|households| households.get(slot.home_slot_index))
                .copied();
            if let Some(person_id) = select_role_candidate(
                settlement,
                current_year,
                slot.role_id,
                preferred_household_id,
                &selected_household_ids,
                &used_people,
            ) {
                used_people.insert(person_id);
                selected_agent_seeds.push(SelectedAgentSeed {
                    new_id: next_agent_id,
                    settlement_index: layout.index,
                    historical_person_id: person_id,
                    role_id: slot.role_id.to_string(),
                    work_building_id: slot.work_building_id,
                    preferred_home_slot_index: slot.home_slot_index,
                    home_building_id: None,
                    home_bed: None,
                });
                next_agent_id += 1;
            }
        }
        let selected_in_settlement = selected_agent_seeds
            .iter()
            .filter(|seed| seed.settlement_index == layout.index)
            .count();
        if selected_in_settlement < 2 {
            let fallback_people = settlement
                .people
                .iter()
                .filter(|person| person.alive && !used_people.contains(&person.id))
                .take(2 - selected_in_settlement)
                .map(|person| person.id)
                .collect::<Vec<_>>();
            for person_id in fallback_people {
                if next_agent_id > config.max_agents as u64 {
                    break;
                }
                used_people.insert(person_id);
                selected_agent_seeds.push(SelectedAgentSeed {
                    new_id: next_agent_id,
                    settlement_index: layout.index,
                    historical_person_id: person_id,
                    role_id: "campones".to_string(),
                    work_building_id: layout.farm_id,
                    preferred_home_slot_index: 0,
                    home_building_id: None,
                    home_bed: None,
                });
                next_agent_id += 1;
            }
        }
    }

    let mut old_to_new_id = HashMap::new();
    for seed in &selected_agent_seeds {
        old_to_new_id.insert(seed.historical_person_id, seed.new_id);
    }

    let mut bed_map = available_beds_by_home(spatial);
    let mut member_ids_by_home: HashMap<BuildingId, Vec<u64>> = HashMap::new();
    for seed in &mut selected_agent_seeds {
        let settlement = &history.settlements[seed.settlement_index];
        let layout = &village_layouts[seed.settlement_index];
        let person = historical_person(settlement, seed.historical_person_id);
        let home_building_id = selected_households
            .get(&(seed.settlement_index, person.household_id))
            .copied()
            .unwrap_or(layout.home_building_ids[seed.preferred_home_slot_index]);
        let bed = bed_map.get_mut(&home_building_id).and_then(|beds| {
            if beds.is_empty() {
                None
            } else {
                Some(beds.remove(0))
            }
        });
        seed.home_building_id = Some(home_building_id);
        seed.home_bed = bed.or_else(|| Some(building_entrance(spatial, home_building_id)));
        member_ids_by_home
            .entry(home_building_id)
            .or_default()
            .push(seed.new_id);
    }

    let mut representative_agent_by_household: HashMap<(usize, u64), u64> = HashMap::new();
    let mut agent_ids_by_historical_household: HashMap<(usize, u64), Vec<u64>> = HashMap::new();
    for seed in &selected_agent_seeds {
        let settlement = &history.settlements[seed.settlement_index];
        let person = historical_person(settlement, seed.historical_person_id);
        representative_agent_by_household
            .entry((seed.settlement_index, person.household_id))
            .or_insert(seed.new_id);
        agent_ids_by_historical_household
            .entry((seed.settlement_index, person.household_id))
            .or_default()
            .push(seed.new_id);
    }

    let mut polity_id_by_settlement = HashMap::new();
    for layout in village_layouts {
        let Some(settlement) = history.settlements.get(layout.index) else {
            continue;
        };
        let polity_id = out.next_polity_id;
        out.next_polity_id += 1;
        polity_id_by_settlement.insert(layout.index, polity_id);
        out.polities.push(Polity {
            id: polity_id,
            name: settlement.polity.name.clone(),
            ruler_agent_id: settlement
                .leader_person_id
                .and_then(|id| old_to_new_id.get(&id).copied()),
            capital_territory_id: None,
            treasury: settlement.polity.treasury.max(0),
            military_readiness: settlement.polity.military_readiness.clamp(0, 100),
        });
    }

    let mut territory_id_by_key: HashMap<(usize, String), TerritoryId> = HashMap::new();
    for layout in village_layouts {
        let Some(settlement) = history.settlements.get(layout.index) else {
            continue;
        };
        let local_polity_id = polity_id_by_settlement[&layout.index];
        let territory_specs = vec![
            (
                "vila_central".to_string(),
                format!("{} - Vila Central", layout.name),
                vec![
                    layout.home_building_ids[0],
                    layout.home_building_ids[1],
                    layout.home_building_ids[2],
                    layout.workshop_id,
                    layout.bakery_id,
                    layout.tavern_id,
                ],
            ),
            (
                "campos".to_string(),
                format!("{} - Campos", layout.name),
                vec![layout.farm_id],
            ),
            (
                "lenhal".to_string(),
                format!("{} - Lenhal", layout.name),
                vec![layout.woodlot_id],
            ),
            (
                "pedreira".to_string(),
                format!("{} - Pedreira", layout.name),
                vec![layout.quarry_id],
            ),
            (
                "civico".to_string(),
                format!("{} - Distrito Civico", layout.name),
                vec![layout.manor_id, layout.guard_post_id],
            ),
        ];
        for (territory_key, territory_name, building_ids) in territory_specs {
            let territory_state = settlement
                .territory_states
                .iter()
                .find(|state| state.key == territory_key);
            let territory_id = out.next_territory_id;
            out.next_territory_id += 1;
            let controller_settlement_id = territory_state
                .map(|state| state.controller_settlement_id)
                .unwrap_or(layout.index);
            let controller_polity_id = polity_id_by_settlement
                .get(&controller_settlement_id)
                .copied()
                .unwrap_or(local_polity_id);
            let mut tile_coords = Vec::new();
            for building_id in &building_ids {
                if let Some(building) = spatial.buildings.iter().find(|b| b.id == *building_id) {
                    tile_coords.extend(building.footprint.iter().copied());
                }
            }
            tile_coords.sort_by_key(|coord| (coord.y, coord.x));
            tile_coords.dedup();
            let pressure = territory_state.map(|state| state.pressure).unwrap_or(20);
            out.territories.push(Territory {
                id: territory_id,
                name: territory_name,
                controller_polity_id: controller_polity_id,
                claimed_by: if controller_polity_id == local_polity_id {
                    vec![local_polity_id]
                } else {
                    vec![local_polity_id, controller_polity_id]
                },
                building_ids,
                tile_coords,
                stability: territory_state
                    .map(|state| state.stability)
                    .unwrap_or(55)
                    .clamp(0, 100),
                strategic_value: territory_state
                    .map(|state| state.strategic_value)
                    .unwrap_or(40)
                    .clamp(0, 100),
                control_pressure: vec![
                    TerritoryControlPressure {
                        polity_id: controller_polity_id,
                        pressure: pressure.clamp(10, 100),
                    },
                    TerritoryControlPressure {
                        polity_id: local_polity_id,
                        pressure: (100 - pressure).clamp(0, 90),
                    },
                ],
                horror: TerritoryHorrorState::default(),
            });
            territory_id_by_key.insert((layout.index, territory_key), territory_id);
        }
    }

    for polity in &mut out.polities {
        let capital = territory_id_by_key
            .iter()
            .find_map(|((settlement_idx, key), id)| {
                (polity_id_by_settlement.get(settlement_idx).copied() == Some(polity.id)
                    && key == "vila_central")
                    .then_some(*id)
            });
        polity.capital_territory_id = capital;
    }

    let mut story_ids_by_settlement: HashMap<usize, Vec<CulturalStoryId>> = HashMap::new();
    let mut next_story_version_id = 1_u64;
    let mut next_tradition_id = 1_u64;
    for layout in village_layouts {
        let Some(settlement) = history.settlements.get(layout.index) else {
            continue;
        };
        for story_seed in settlement.story_seeds.iter().take(4) {
            let story_id = out.next_cultural_story_id;
            out.next_cultural_story_id += 1;
            let cited_agent_ids = story_seed
                .cited_names
                .iter()
                .filter_map(|name| {
                    selected_agent_seeds.iter().find_map(|seed| {
                        let person = historical_person(
                            &history.settlements[seed.settlement_index],
                            seed.historical_person_id,
                        );
                        (seed.settlement_index == layout.index && person.name == *name)
                            .then_some(seed.new_id)
                    })
                })
                .collect::<Vec<_>>();
            let associated_territory_id = territory_id_by_key
                .get(&(layout.index, "vila_central".to_string()))
                .copied();
            out.cultural_stories.push(CulturalStory {
                id: story_id,
                title: story_seed.title.clone(),
                narrative_core: story_seed.summary.clone(),
                origin_kind: story_seed.kind,
                theme: story_seed
                    .tags
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "historia".to_string()),
                moral: story_seed.moral.clone(),
                cited_agent_ids,
                associated_building_id: Some(layout.manor_id),
                associated_territory_id,
                source_event_summaries: vec![story_seed.summary.clone()],
                origin_generation: story_seed.origin_generation,
                cultural_strength: (55 + story_seed.tags.len() as i32 * 5).clamp(25, 100),
                stability: 45,
                distortion: 15,
                status: StoryStatus::Estavel,
                created_day: 1,
                last_told_tick: 0,
                tell_count: 1,
            });
            out.story_versions.push(StoryVersion {
                id: next_story_version_id,
                story_id,
                short_version: story_seed.summary.clone(),
                author_agent_id: None,
                transmitter_agent_id: settlement
                    .leader_person_id
                    .and_then(|id| old_to_new_id.get(&id).copied()),
                generation: story_seed.origin_generation,
                tone: story_seed
                    .tags
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "solene".to_string()),
                distortion: 10,
                cultural_tags: story_seed.tags.clone(),
                created_day: 1,
                created_tick: 0,
            });
            next_story_version_id += 1;
            if matches!(
                story_seed.kind,
                CulturalStoryKind::Fundacao
                    | CulturalStoryKind::CantoDeGuerra
                    | CulturalStoryKind::Martirio
            ) {
                out.cultural_traditions.push(CulturalTradition {
                    id: next_tradition_id,
                    story_id,
                    name: format!("Memoria de {}", story_seed.title),
                    associated_building_id: Some(layout.manor_id),
                    associated_faction_id: None,
                    recurrence_days: 12,
                    strength: 40,
                    last_observed_day: 1,
                });
                next_tradition_id += 1;
            }
            story_ids_by_settlement
                .entry(layout.index)
                .or_default()
                .push(story_id);
        }
    }

    let mut role_home_ids: HashMap<(usize, String), Vec<BuildingId>> = HashMap::new();
    for seed in &selected_agent_seeds {
        if let Some(home_building_id) = seed.home_building_id {
            role_home_ids
                .entry((seed.settlement_index, seed.role_id.clone()))
                .or_default()
                .push(home_building_id);
        }
    }

    let mut next_memory_id = 1_u64;
    for seed in &selected_agent_seeds {
        let settlement = &history.settlements[seed.settlement_index];
        let layout = &village_layouts[seed.settlement_index];
        let person = historical_person(settlement, seed.historical_person_id);
        let household = settlement
            .households
            .iter()
            .find(|household| household.id == person.household_id)
            .expect("historical household missing for person");
        let age = historical_age(person, current_year).max(16) as u32;
        let home_building_id = seed.home_building_id;
        let home_bed = seed.home_bed;
        let position =
            home_bed.unwrap_or_else(|| building_entrance(spatial, layout.home_building_ids[0]));
        let relations = HashMap::new();
        let long_term_plan = derive_long_term_plan(&seed.role_id, household, Some(&layout.name));
        let memories = vec![
            AgentMemory {
                id: next_memory_id,
                day: 1,
                tick: 0,
                kind: MemoryKind::Fact,
                summary: format!(
                    "Cresceu em {} sob a linhagem de {}.",
                    layout.name, household.name
                ),
                details: format!(
                    "{} carrega um passado de {} anos de consolidacao local.",
                    person.name, history.years_simulated
                ),
                emotional_weight: 18,
                about: Vec::new(),
                tags: vec!["historia".to_string(), "linhagem".to_string()],
            },
            AgentMemory {
                id: next_memory_id + 1,
                day: 1,
                tick: 0,
                kind: MemoryKind::Reflection,
                summary: long_term_plan.clone(),
                details: format!("Plano herdado da pre-historia: {}", long_term_plan),
                emotional_weight: 12,
                about: Vec::new(),
                tags: vec!["plano".to_string(), "fundacao".to_string()],
            },
        ];
        next_memory_id += 2;
        let story_beliefs = story_ids_by_settlement
            .get(&seed.settlement_index)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|story_id| StoryBelief {
                story_id,
                belief: 65,
                emotional_attachment: (30 + person.trauma / 2).clamp(10, 90),
                moral_interpretation: story_moral_hint(&seed.role_id),
                heard_from: settlement
                    .leader_person_id
                    .and_then(|id| old_to_new_id.get(&id).copied()),
                first_heard_tick: 0,
                last_heard_tick: 0,
            })
            .collect::<Vec<_>>();
        let parents = [person.father_id, person.mother_id]
            .into_iter()
            .flatten()
            .filter_map(|id| old_to_new_id.get(&id).copied())
            .collect::<Vec<_>>();
        let children = person
            .children_ids
            .iter()
            .filter_map(|id| old_to_new_id.get(id).copied())
            .collect::<Vec<_>>();
        let craft_proficiencies = build_craft_proficiencies(person, &seed.role_id, household);
        let mut inventory_item_ids = Vec::new();
        let mut equipped_items = HashMap::new();
        for resource_id in seeded_equipment_resource_ids(&seed.role_id, household, person) {
            if let Some(item) = create_seeded_item_instance(
                catalog,
                &mut out.next_item_instance_id,
                resource_id,
                seed.new_id,
                home_building_id,
                Some(seed.new_id),
                Some(person.name.clone()),
                &craft_proficiencies,
            ) {
                if let Some(resource) = catalog
                    .resources
                    .iter()
                    .find(|entry| entry.id == item.resource_id)
                {
                    for slot in &resource.equip_slot_preferences {
                        equipped_items.entry(*slot).or_insert(item.id);
                    }
                }
                inventory_item_ids.push(item.id);
                out.item_instances.push(item);
            }
        }
        out.agents.push(AgentSnapshot {
            id: seed.new_id,
            name: person.name.clone(),
            role_id: seed.role_id.clone(),
            home_building_id,
            work_building_id: Some(seed.work_building_id),
            home_bed,
            profile: build_profile(person, household, &seed.role_id),
            state: build_agent_state(person, household, &seed.role_id),
            life_status: AgentLifeStatus::Vivo,
            injury: initial_historical_injury(person, household),
            institutional_perception: build_institutional_perception(
                person,
                household,
                &seed.role_id,
                settlement,
            ),
            psychological_state: build_psychology(
                person,
                household,
                &seed.role_id,
                Some(&layout.name),
                1,
            ),
            horror: HorrorExposure::default(),
            rumor_beliefs: Vec::new(),
            story_beliefs,
            relations,
            memories,
            inventory: Vec::new(),
            inventory_item_ids,
            equipped_items,
            craft_proficiencies,
            position,
            destination: None,
            destination_label: None,
            planned_path: Vec::new(),
            current_building_id: Some(home_building_id.unwrap_or(layout.home_building_ids[0])),
            current_room_id: room_at_position(spatial, position),
            active_conversation_id: None,
            conversation_participant_ids: Vec::new(),
            last_social_act: None,
            social_cooldown_until: 0,
            last_intent: None,
            task_queue: Vec::new(),
            last_thought: format!(
                "{} observa o mundo que herdou de cem anos de disputas e colheitas.",
                person.name
            ),
            llm_cooldown_until: 0,
            llm_calls: 0,
            active_economic_task_id: None,
            carrying: Vec::new(),
            carrying_capacity: 12,
            next_reconsideration_tick: 0,
            blocked_ticks: 0,
            last_cognition_trigger: Some("bootstrap_historico".to_string()),
            last_social_opportunity_signature: None,
            last_deliberation_hunger: 0,
            last_deliberation_energy: 0,
            last_deliberation_health: 0,
            last_deliberation_stress: 0,
            trauma_tracker: TraumaTracker::default(),
            age,
            parents,
            children,
            spouse: person
                .spouse_id
                .and_then(|id| old_to_new_id.get(&id).copied()),
            gender: person.sex.clone(),
        });
    }

    populate_relations(&mut out.agents);

    for layout in village_layouts {
        let Some(settlement) = history.settlements.get(layout.index) else {
            continue;
        };
        let primary_households = settlement_primary_households
            .get(&layout.index)
            .cloned()
            .unwrap_or_default();
        for historical_household_id in primary_households {
            let Some(home_building_id) = selected_households
                .get(&(layout.index, historical_household_id))
                .copied()
            else {
                continue;
            };
            let member_ids = member_ids_by_home
                .get(&home_building_id)
                .cloned()
                .unwrap_or_default();
            if member_ids.is_empty() {
                continue;
            }
            let household = settlement
                .households
                .iter()
                .find(|household| household.id == historical_household_id)
                .expect("selected historical household missing");
            out.households.push(HouseholdEconomy {
                id: home_building_id,
                name: household.name.clone(),
                member_ids,
                treasury: household.wealth.max(0),
                pantry: initial_household_pantry(household),
                reserved_food: Vec::new(),
                minimum_food_units: 4.max(
                    out.agents
                        .iter()
                        .filter(|agent| agent.home_building_id == Some(home_building_id))
                        .count() as i32
                        * 2,
                ),
                pending_payments: Vec::new(),
                scarcity_pressure: (household.hardship * 4).clamp(0, 100),
                food_crisis_level: household.hardship.clamp(0, 10) as u8,
                reserved_food_workers: 0,
                last_food_shortage_tick: 0,
                tax_arrears: household.feudal_arrears.max(0),
                last_tax_paid_day: 0,
                direct_lord_agent_id: settlement
                    .leader_person_id
                    .and_then(|id| old_to_new_id.get(&id).copied()),
                feudal_tribute_due: household.feudal_arrears.max(0),
                corvee_days_due: household.hardship.clamp(0, 6),
                levy_service_due: (household.social_rank / 15).clamp(0, 4),
                feudal_arrears: household.feudal_arrears.max(0),
            });
        }
    }

    let mut next_establishment_id = 1_u64;
    let mut establishment_id_by_building = HashMap::new();
    for layout in village_layouts {
        let Some(settlement) = history.settlements.get(layout.index) else {
            continue;
        };
        for (establishment_type_id, building_id) in [
            ("solar", layout.manor_id),
            ("posto_guarda", layout.guard_post_id),
            ("forja", layout.workshop_id),
            ("padaria", layout.bakery_id),
            ("taverna", layout.tavern_id),
            ("fazenda", layout.farm_id),
            ("lenhal", layout.woodlot_id),
            ("pedreira", layout.quarry_id),
        ] {
            let Some(def) = establishment_def(catalog, establishment_type_id) else {
                continue;
            };
            let owner_household_ids = owner_households_for_establishment(
                layout.index,
                establishment_type_id,
                &role_home_ids,
            );
            let stock = initial_establishment_stock(def, settlement);
            let establishment = EstablishmentEconomy {
                id: next_establishment_id,
                building_id: Some(building_id),
                name: building_name(spatial, building_id),
                establishment_type_id: establishment_type_id.to_string(),
                location_kind: def.location_kind,
                owner_household_ids,
                storage_fixture_id: first_storage_fixture(spatial, building_id),
                cash: initial_establishment_cash(def, settlement),
                item_stock_ids: Vec::new(),
                stock_targets: def.stock_targets.clone(),
                posted_prices: build_posted_prices(catalog, &def.stock_targets, &stock),
                stock,
                wage_per_shift: def.wage_per_shift,
                tool_wear: 0,
                public_service: def.public_service,
            };
            establishment_id_by_building.insert(building_id, establishment.id);
            out.establishments.push(establishment);
            next_establishment_id += 1;
        }
    }

    let total_public_treasury = out.polities.iter().map(|polity| polity.treasury).sum();
    out.village_economy = VillageEconomy {
        public_treasury: total_public_treasury,
        daily_household_tax: history
            .settlements
            .first()
            .map(|settlement| {
                (settlement
                    .households
                    .iter()
                    .map(|household| household.feudal_arrears.max(0))
                    .sum::<i32>()
                    / settlement.households.len().max(1) as i32)
                    .clamp(2, 8)
            })
            .unwrap_or(4),
        inter_village_trade_coord: TileCoord { x: 2, y: 2 },
        base_prices: catalog
            .resources
            .iter()
            .map(|resource| PostedPrice {
                resource_id: resource.id.clone(),
                unit_price: resource.base_price,
            })
            .collect(),
        scarcity_metrics: build_scarcity_metrics(&out.establishments, &out.households),
    };

    for layout in village_layouts {
        let Some(settlement) = history.settlements.get(layout.index) else {
            continue;
        };
        let polity_id = polity_id_by_settlement[&layout.index];
        let mut lord_agent_id = settlement
            .leader_person_id
            .and_then(|id| old_to_new_id.get(&id).copied());
        if lord_agent_id.is_none() {
            lord_agent_id = selected_agent_seeds
                .iter()
                .find(|seed| seed.settlement_index == layout.index)
                .map(|seed| seed.new_id);
        }
        let mut field_vassal_agent_id = settlement
            .field_vassal_person_id
            .and_then(|id| old_to_new_id.get(&id).copied())
            .or_else(|| role_agent_id(layout.index, "campones", &selected_agent_seeds));
        if field_vassal_agent_id.is_none() || field_vassal_agent_id == lord_agent_id {
            field_vassal_agent_id = selected_agent_seeds
                .iter()
                .find(|seed| {
                    seed.settlement_index == layout.index && Some(seed.new_id) != lord_agent_id
                })
                .map(|seed| seed.new_id)
                .or(field_vassal_agent_id);
        }
        let steward_agent_id = settlement
            .steward_person_id
            .and_then(|id| old_to_new_id.get(&id).copied())
            .or_else(|| role_agent_id(layout.index, "taverneiro", &selected_agent_seeds));
        let captain_agent_id = settlement
            .captain_person_id
            .and_then(|id| old_to_new_id.get(&id).copied())
            .or_else(|| role_agent_id(layout.index, "guarda", &selected_agent_seeds));

        let central_territory_id = territory_id_by_key[&(layout.index, "vila_central".to_string())];
        let fields_territory_id = territory_id_by_key[&(layout.index, "campos".to_string())];
        let civico_territory_id = territory_id_by_key[&(layout.index, "civico".to_string())];

        let domain_holding_id = out.next_estate_holding_id;
        out.next_estate_holding_id += 1;
        out.estate_holdings.push(EstateHolding {
            id: domain_holding_id,
            name: format!("Dominio de {}", layout.name),
            holder_kind: EstateHolderKind::Agent,
            holder_agent_id: lord_agent_id,
            holder_household_id: lord_agent_id.and_then(|agent_id| {
                out.agents
                    .iter()
                    .find(|agent| agent.id == agent_id)
                    .and_then(|agent| agent.home_building_id)
            }),
            holder_polity_id: Some(polity_id),
            territory_id: central_territory_id,
            building_ids: vec![layout.manor_id, layout.guard_post_id],
            establishment_ids: vec![
                establishment_id_by_building[&layout.manor_id],
                establishment_id_by_building[&layout.guard_post_id],
            ],
            annualized_value: settlement.polity.treasury.max(20),
            tribute_share_percent: 25,
            labor_obligation_days: 2,
            military_obligation: 2,
        });

        let field_holding_id = out.next_estate_holding_id;
        out.next_estate_holding_id += 1;
        out.estate_holdings.push(EstateHolding {
            id: field_holding_id,
            name: format!("Campos de {}", layout.name),
            holder_kind: if field_vassal_agent_id.is_some() {
                EstateHolderKind::Agent
            } else {
                EstateHolderKind::Household
            },
            holder_agent_id: field_vassal_agent_id,
            holder_household_id: field_vassal_agent_id
                .and_then(|agent_id| {
                    out.agents
                        .iter()
                        .find(|agent| agent.id == agent_id)
                        .and_then(|agent| agent.home_building_id)
                })
                .or_else(|| {
                    role_home_ids
                        .get(&(layout.index, "campones".to_string()))
                        .and_then(|homes| homes.first().copied())
                }),
            holder_polity_id: Some(polity_id),
            territory_id: fields_territory_id,
            building_ids: vec![layout.farm_id, layout.woodlot_id, layout.quarry_id],
            establishment_ids: vec![
                establishment_id_by_building[&layout.farm_id],
                establishment_id_by_building[&layout.woodlot_id],
                establishment_id_by_building[&layout.quarry_id],
            ],
            annualized_value: settlement
                .households
                .iter()
                .map(|household| household.wealth + household.grain)
                .sum::<i32>()
                .clamp(25, 180),
            tribute_share_percent: 18,
            labor_obligation_days: 3,
            military_obligation: 1,
        });

        let lord_title_id = out.next_feudal_title_id;
        out.next_feudal_title_id += 1;
        out.feudal_titles.push(FeudalTitle {
            id: lord_title_id,
            name: format!("Senhor de {}", layout.name),
            rank: FeudalRank::Senhor,
            holder_agent_id: lord_agent_id,
            polity_id: Some(polity_id),
            territory_id: Some(central_territory_id),
            holding_id: Some(domain_holding_id),
            suzerain_title_id: None,
            succession_rule: SuccessionRule::HerdeiroDireto,
            legitimacy: settlement
                .leader_person_id
                .and_then(|id| settlement.people.iter().find(|person| person.id == id))
                .map(|leader| leader.legitimacy)
                .unwrap_or(40)
                .clamp(0, 100),
            precedence: 80,
            active: true,
        });
        let vassal_title_id = out.next_feudal_title_id;
        out.next_feudal_title_id += 1;
        out.feudal_titles.push(FeudalTitle {
            id: vassal_title_id,
            name: format!("Vassalo dos Campos de {}", layout.name),
            rank: FeudalRank::Cavaleiro,
            holder_agent_id: field_vassal_agent_id,
            polity_id: Some(polity_id),
            territory_id: Some(fields_territory_id),
            holding_id: Some(field_holding_id),
            suzerain_title_id: Some(lord_title_id),
            succession_rule: SuccessionRule::NomeacaoDoSuserano,
            legitimacy: 45,
            precedence: 55,
            active: field_vassal_agent_id.is_some(),
        });

        if let (Some(suzerain_agent_id), Some(vassal_agent_id)) =
            (lord_agent_id, field_vassal_agent_id)
        {
            if suzerain_agent_id != vassal_agent_id {
                let unpaid_tribute = settlement
                    .recent_feudal_duties
                    .iter()
                    .filter(|duty| {
                        duty.kind == crate::world_history::HistoricalFeudalDutyKind::Tribute
                            && !duty.complied
                    })
                    .count() as i32;
                out.feudal_contracts.push(FeudalContract {
                    id: out.next_feudal_contract_id,
                    suzerain_agent_id,
                    vassal_agent_id,
                    territory_id: Some(fields_territory_id),
                    holding_id: Some(field_holding_id),
                    tribute_due_per_day: 2,
                    levy_duty: 1,
                    judicial_aid_duty: 1,
                    maintenance_duty: 1,
                    loyalty: (45 - unpaid_tribute * 6).clamp(5, 80),
                    coercion: (25 + unpaid_tribute * 5).clamp(5, 90),
                    perceived_legitimacy: (35 - unpaid_tribute * 7).clamp(-40, 80),
                    status: if unpaid_tribute >= 2 {
                        FeudalContractStatus::Breached
                    } else {
                        FeudalContractStatus::Active
                    },
                    last_updated_day: 1,
                });
                out.next_feudal_contract_id += 1;
            }
        }

        for (kind, name, holder_agent_id, territory_id, title_id, authority_score) in [
            (
                AuthorityOfficeKind::CapitaoDaGuarda,
                format!("Capitao da Guarda de {}", layout.name),
                captain_agent_id,
                Some(civico_territory_id),
                Some(lord_title_id),
                58,
            ),
            (
                AuthorityOfficeKind::AdministradorDoSolar,
                format!("Administrador do Solar de {}", layout.name),
                steward_agent_id.or(lord_agent_id),
                Some(central_territory_id),
                Some(lord_title_id),
                46,
            ),
        ] {
            out.authority_offices.push(AuthorityOffice {
                id: out.next_authority_office_id,
                kind,
                name,
                granter_agent_id: lord_agent_id,
                holder_agent_id,
                territory_id,
                title_id,
                active: holder_agent_id.is_some(),
                authority_score,
            });
            out.next_authority_office_id += 1;
        }

        for (agent_id, territory_id, title_id, formal, material, coercive, legitimacy, summary) in [
            (
                lord_agent_id,
                Some(central_territory_id),
                Some(lord_title_id),
                75,
                settlement.polity.treasury.clamp(20, 100),
                settlement.polity.military_readiness.clamp(10, 100),
                50,
                format!(
                    "Manda por titulo, caixa e controle do solar em {}.",
                    layout.name
                ),
            ),
            (
                captain_agent_id,
                Some(civico_territory_id),
                None,
                40,
                28,
                70,
                35,
                format!("Controla a coercao imediata da guarda em {}.", layout.name),
            ),
            (
                field_vassal_agent_id,
                Some(fields_territory_id),
                Some(vassal_title_id),
                45,
                settlement
                    .households
                    .iter()
                    .map(|household| household.grain + household.wealth / 2)
                    .sum::<i32>()
                    .clamp(15, 95),
                25,
                38,
                format!(
                    "Sustenta a base alimentar e o trabalho rural de {}.",
                    layout.name
                ),
            ),
        ] {
            if let Some(agent_id) = agent_id {
                out.power_centers.push(PowerCenter {
                    id: out.next_power_center_id,
                    territory_id,
                    title_id,
                    agent_id: Some(agent_id),
                    formal_authority: formal,
                    material_power: material,
                    coercive_power: coercive,
                    legitimacy,
                    stability: settlement
                        .territory_states
                        .iter()
                        .map(|territory| territory.stability)
                        .sum::<i32>()
                        / settlement.territory_states.len().max(1) as i32,
                    summary,
                });
                out.next_power_center_id += 1;
            }
        }

        if let Some(recent_succession) = &settlement.recent_succession {
            if recent_succession.conflict_score >= 18 {
                out.succession_crises.push(SuccessionCrisis {
                    id: out.next_succession_crisis_id,
                    title_id: lord_title_id,
                    territory_id: Some(central_territory_id),
                    claimant_ids: recent_succession
                        .claimant_person_ids
                        .iter()
                        .filter_map(|id| old_to_new_id.get(id).copied())
                        .collect(),
                    recognized_heir_id: recent_succession
                        .recognized_heir_id
                        .and_then(|id| old_to_new_id.get(&id).copied()),
                    usurper_id: None,
                    status: SuccessionCrisisStatus::Open,
                    legitimacy_gap: recent_succession.legitimacy_gap,
                    conflict_score: recent_succession.conflict_score,
                    opened_day: 1,
                    resolved_day: None,
                    summary: recent_succession.summary.clone(),
                });
                out.next_succession_crisis_id += 1;
            }
        }
    }

    if out.feudal_contracts.is_empty() && out.agents.len() >= 2 {
        let suzerain_agent_id = out.agents[0].id;
        let vassal_agent_id = out
            .agents
            .iter()
            .find(|agent| agent.id != suzerain_agent_id)
            .map(|agent| agent.id)
            .unwrap_or(suzerain_agent_id);
        if suzerain_agent_id != vassal_agent_id {
            out.feudal_contracts.push(FeudalContract {
                id: out.next_feudal_contract_id,
                suzerain_agent_id,
                vassal_agent_id,
                territory_id: out.territories.first().map(|territory| territory.id),
                holding_id: out.estate_holdings.first().map(|holding| holding.id),
                tribute_due_per_day: 2,
                levy_duty: 1,
                judicial_aid_duty: 1,
                maintenance_duty: 1,
                loyalty: 42,
                coercion: 25,
                perceived_legitimacy: 35,
                status: FeudalContractStatus::Active,
                last_updated_day: 1,
            });
            out.next_feudal_contract_id += 1;
        }
    }

    for layout in village_layouts {
        let Some(settlement) = history.settlements.get(layout.index) else {
            continue;
        };
        let polity_id = polity_id_by_settlement[&layout.index];
        let central_territory_id = territory_id_by_key[&(layout.index, "vila_central".to_string())];
        let civic_territory_id = territory_id_by_key[&(layout.index, "civico".to_string())];
        let issuer_agent_id = settlement
            .leader_person_id
            .and_then(|id| old_to_new_id.get(&id).copied());
        let settlement_agent_ids = selected_agent_seeds
            .iter()
            .filter(|seed| seed.settlement_index == layout.index)
            .map(|seed| seed.new_id)
            .collect::<Vec<_>>();
        let dissident_agent_id = settlement_agent_ids
            .iter()
            .copied()
            .find(|id| Some(*id) != issuer_agent_id)
            .or(issuer_agent_id)
            .unwrap_or(0);

        if settlement.local_norms.justice_severity != JusticeSeverity::Normal {
            out.policy_acts.push(PolicyAct {
                id: out.next_policy_act_id,
                agenda_tag: "justica_historica".to_string(),
                summary: format!(
                    "{} manteve justica {} ao fim do seculo.",
                    layout.name,
                    settlement.local_norms.justice_severity.as_str()
                ),
                issuer_agent_id,
                issuer_polity_id: Some(polity_id),
                authority: PolicyAuthority::LocalLord,
                scope: PolicyScope::Territory(civic_territory_id),
                target: PolicyTarget::None,
                effects: Vec::new(),
                legitimacy: 35,
                enforcement: 50,
                resistance: settlement
                    .households
                    .iter()
                    .map(|household| household.rage)
                    .sum::<i32>()
                    / settlement.households.len().max(1) as i32,
                status: PolicyActStatus::Active,
                issued_day: 1,
                issued_tick: 0,
                expires_day: None,
            });
            out.next_policy_act_id += 1;
        }
        if settlement.local_norms.rationing_policy != RationingPolicy::Balanced {
            out.policy_acts.push(PolicyAct {
                id: out.next_policy_act_id,
                agenda_tag: "racionamento_historico".to_string(),
                summary: format!(
                    "{} manteve racionamento {} ao fim do seculo.",
                    layout.name,
                    settlement.local_norms.rationing_policy.as_str()
                ),
                issuer_agent_id,
                issuer_polity_id: Some(polity_id),
                authority: PolicyAuthority::LocalLord,
                scope: PolicyScope::Territory(central_territory_id),
                target: PolicyTarget::None,
                effects: vec![PolicyEffect::RationingRule {
                    policy: settlement.local_norms.rationing_policy,
                    energy_gain_percent: 100,
                }],
                legitimacy: 35,
                enforcement: 50,
                resistance: settlement
                    .households
                    .iter()
                    .map(|household| household.rage)
                    .sum::<i32>()
                    / settlement.households.len().max(1) as i32,
                status: PolicyActStatus::Active,
                issued_day: 1,
                issued_tick: 0,
                expires_day: None,
            });
            out.next_policy_act_id += 1;
        }
        for decree in settlement.recent_decrees.iter().take(3) {
            let (scope, target, effects, summary) = policy_from_tag(
                &decree.agenda_tag,
                layout,
                settlement,
                polity_id,
                &territory_id_by_key,
            );
            let scope = decree
                .target_territory_key
                .as_ref()
                .and_then(|key| {
                    territory_id_by_key
                        .get(&(layout.index, key.clone()))
                        .copied()
                })
                .map(PolicyScope::Territory)
                .unwrap_or(scope);
            out.policy_acts.push(PolicyAct {
                id: out.next_policy_act_id,
                agenda_tag: decree.agenda_tag.clone(),
                summary: if decree.summary.is_empty() {
                    format!("{} Valor proposto: {}.", summary, decree.proposed_value)
                } else {
                    format!(
                        "{} Valor proposto: {}.",
                        decree.summary, decree.proposed_value
                    )
                },
                issuer_agent_id,
                issuer_polity_id: Some(polity_id),
                authority: PolicyAuthority::LocalLord,
                scope,
                target,
                effects,
                legitimacy: decree.legitimacy,
                enforcement: decree.enforcement,
                resistance: settlement
                    .recent_pressures
                    .iter()
                    .map(|pressure| pressure.intensity)
                    .sum::<i32>()
                    .clamp(0, 100),
                status: PolicyActStatus::Active,
                issued_day: 1,
                issued_tick: 0,
                expires_day: None,
            });
            out.next_policy_act_id += 1;
        }
        for tag in settlement.recent_policy_tags.iter().take(2) {
            if settlement
                .recent_decrees
                .iter()
                .any(|decree| decree.agenda_tag == *tag)
            {
                continue;
            }
            let (scope, target, effects, summary) =
                policy_from_tag(tag, layout, settlement, polity_id, &territory_id_by_key);
            out.policy_acts.push(PolicyAct {
                id: out.next_policy_act_id,
                agenda_tag: tag.clone(),
                summary,
                issuer_agent_id,
                issuer_polity_id: Some(polity_id),
                authority: PolicyAuthority::LocalLord,
                scope,
                target,
                effects,
                legitimacy: 28,
                enforcement: 44,
                resistance: settlement
                    .recent_pressures
                    .iter()
                    .map(|pressure| pressure.intensity)
                    .sum::<i32>()
                    .clamp(0, 100),
                status: PolicyActStatus::Active,
                issued_day: 1,
                issued_tick: 0,
                expires_day: None,
            });
            out.next_policy_act_id += 1;
        }
        for pressure in &settlement.recent_pressures {
            let domain = pressure_domain(&pressure.agenda_tag);
            let household_id = settlement_primary_households
                .get(&layout.index)
                .and_then(|households| households.first().copied())
                .and_then(|historical_household_id| {
                    selected_households
                        .get(&(layout.index, historical_household_id))
                        .copied()
                });
            out.political_pressures.push(PoliticalPressure {
                actor_id: issuer_agent_id.unwrap_or(0),
                household_id,
                agenda_tag: pressure.agenda_tag.clone(),
                domain,
                proposed_value: pressure.proposed_value.clone(),
                intensity: pressure.intensity,
                reason: pressure.reason.clone(),
                day: 1,
                tick: 0,
            });

            let issue_id = out.next_political_issue_id;
            out.next_political_issue_id += 1;
            out.political_issues.push(PoliticalIssue {
                id: issue_id,
                agenda_tag: pressure.agenda_tag.clone(),
                domain,
                proposed_value: pressure.proposed_value.clone(),
                summary: pressure.reason.clone(),
                proposed_by: Some(dissident_agent_id),
                support_score: pressure.intensity,
                opposition_score: 0,
                supporter_ids: vec![dissident_agent_id],
                opposer_ids: Vec::new(),
                status: PoliticalIssueStatus::Open,
                opened_day: 1,
                resolved_day: None,
            });

            let faction_id = out.next_political_faction_id;
            out.next_political_faction_id += 1;
            let objective = match pressure.agenda_tag.as_str() {
                "motim_comida" => Some(FactionObjective::FoodRiot {
                    barn_building_id: layout.farm_id,
                    target_grains: pressure.intensity.clamp(4, 20),
                    grains_stolen: 0,
                }),
                "boicote_imposto" => Some(FactionObjective::TaxBoycott { day_activated: 1 }),
                "depor_lider" => issuer_agent_id
                    .map(|leader_agent_id| FactionObjective::DeposeLeader { leader_agent_id }),
                _ => None,
            };
            out.political_factions.push(PoliticalFaction {
                id: faction_id,
                name: format!("Faccao {} de {}", pressure.agenda_tag, layout.name),
                agenda_tag: pressure.agenda_tag.clone(),
                domain,
                proposed_value: pressure.proposed_value.clone(),
                founder_id: dissident_agent_id,
                member_ids: settlement_agent_ids.iter().copied().take(3).collect(),
                influence: (20 + pressure.intensity * 2).clamp(10, 100),
                support_issue_ids: vec![issue_id],
                opposition_issue_ids: Vec::new(),
                objective,
                is_action_active: pressure.intensity >= 18,
                rage: (pressure.intensity * 3).clamp(0, 100),
            });
        }

        for justice in settlement.recent_justice_cases.iter().take(3) {
            out.crime_cases.push(CrimeCase {
                id: out.next_crime_case_id,
                crime_type: if justice.punitive {
                    CrimeType::Theft
                } else {
                    CrimeType::Robbery
                },
                victim_id: justice.victim_household_id.and_then(|household_id| {
                    representative_agent_by_household
                        .get(&(layout.index, household_id))
                        .copied()
                }),
                suspect_id: justice.suspect_household_id.and_then(|household_id| {
                    representative_agent_by_household
                        .get(&(layout.index, household_id))
                        .copied()
                }),
                witnesses: settlement_agent_ids.iter().copied().take(2).collect(),
                evidence: vec![justice.summary.clone()],
                severity: justice.severity,
                confidence: if justice.proven { 85 } else { 45 },
                status: if justice.punitive {
                    CrimeCaseStatus::Punished
                } else if justice.proven {
                    CrimeCaseStatus::Proven
                } else {
                    CrimeCaseStatus::Investigating
                },
                sentence: if justice.punitive {
                    if justice.severity >= 7 {
                        SentenceKind::Corporal
                    } else {
                        SentenceKind::Detention
                    }
                } else {
                    SentenceKind::None
                },
                opened_day: 1,
                opened_tick: 0,
                summary: justice.summary.clone(),
            });
            out.next_crime_case_id += 1;
        }

        if let Some(insurrection) = &settlement.recent_insurrection {
            let matching_factions = out
                .political_factions
                .iter()
                .filter(|faction| {
                    faction.agenda_tag == insurrection.agenda_tag
                        && faction
                            .member_ids
                            .iter()
                            .any(|id| settlement_agent_ids.contains(id))
                })
                .map(|faction| faction.id)
                .collect::<Vec<_>>();
            let rebel_polity_id = if insurrection.stage == InsurrectionStage::CivilWar {
                let polity_id = out.next_polity_id;
                out.next_polity_id += 1;
                out.polities.push(Polity {
                    id: polity_id,
                    name: format!("Rebeldes de {}", layout.name),
                    ruler_agent_id: Some(dissident_agent_id),
                    capital_territory_id: Some(central_territory_id),
                    treasury: 0,
                    military_readiness: (insurrection.popular_support / 2).clamp(10, 70),
                });
                Some(polity_id)
            } else {
                None
            };
            out.insurrections.push(InsurrectionState {
                id: out.next_insurrection_id,
                faction_ids: matching_factions,
                target_polity_id: polity_id,
                rebel_polity_id,
                target_territory_id: territory_id_by_key
                    [&(layout.index, insurrection.target_territory_key.clone())],
                popular_support: insurrection.popular_support,
                repression: insurrection.repression,
                stage: insurrection.stage,
                status: InsurrectionStatus::Active,
                linked_war_id: None,
                started_day: 1,
                ended_day: None,
                summary: insurrection.summary.clone(),
            });
            out.next_insurrection_id += 1;
        }

        for construction in settlement
            .recent_constructions
            .iter()
            .filter(|construction| !construction.completed)
        {
            let anchor = building_entrance(spatial, layout.manor_id);
            out.construction_projects.push(ConstructionProject {
                id: out.next_construction_project_id,
                establishment_type_id: construction.establishment_type_id.clone(),
                building_name: construction.summary.clone(),
                planned_footprint: vec![anchor],
                entrance: anchor,
                materials_required: Vec::new(),
                materials_delivered: Vec::new(),
                labor_required: 10,
                labor_done: 0,
                status: ConstructionStatus::Planned,
                priority: 60,
                systemic_reason: construction.reason.clone(),
                resulting_building_id: None,
                funding_polity_id: Some(polity_id),
            });
            out.next_construction_project_id += 1;
        }
    }

    for i in 0..out.polities.len() {
        for j in (i + 1)..out.polities.len() {
            let polity_a = out.polities[i].id;
            let polity_b = out.polities[j].id;
            let relevant_war = history.wars.iter().find(|war| {
                let attacker = polity_id_by_settlement[&war.attacker_settlement_id];
                let defender = polity_id_by_settlement[&war.defender_settlement_id];
                (attacker == polity_a && defender == polity_b)
                    || (attacker == polity_b && defender == polity_a)
            });
            let stance = match relevant_war {
                Some(war) if war.ended_year.is_none() => ForeignStance::AtWar,
                Some(_) => ForeignStance::Rival,
                None => ForeignStance::Neutral,
            };
            out.foreign_relations.push(ForeignRelation {
                id: out.next_foreign_relation_id,
                polity_a,
                polity_b,
                stance,
                trust: if relevant_war.is_some() { -25 } else { 8 },
                fear: relevant_war
                    .map(|war| (war.attacker_score.max(war.defender_score) / 2).clamp(0, 60))
                    .unwrap_or(4),
                grievances: relevant_war
                    .map(|war| vec![war.summary.clone()])
                    .unwrap_or_default(),
                treaty_policy_act_ids: Vec::new(),
            });
            out.next_foreign_relation_id += 1;
        }
    }

    for war in history.wars.iter().filter(|war| war.ended_year.is_none()) {
        let attacker_polity_id = polity_id_by_settlement[&war.attacker_settlement_id];
        let defender_polity_id = polity_id_by_settlement[&war.defender_settlement_id];
        let target_territory_ids = out
            .territories
            .iter()
            .filter(|territory| territory.controller_polity_id == defender_polity_id)
            .take(2)
            .map(|territory| territory.id)
            .collect::<Vec<_>>();
        out.wars.push(WarState {
            id: out.next_war_id,
            attacker_polity_id,
            defender_polity_id,
            target_territory_ids,
            attacker_score: war.attacker_score.clamp(0, 100),
            defender_score: war.defender_score.clamp(0, 100),
            stage: war.stage,
            status: WarStatus::Active,
            winner_polity_id: None,
            started_day: 1,
            ended_day: None,
            summary: format!("{} Inicio: ano {}.", war.summary, war.started_year),
        });
        out.next_war_id += 1;
    }

    for layout in village_layouts {
        let Some(settlement) = history.settlements.get(layout.index) else {
            continue;
        };
        let polity_id = polity_id_by_settlement[&layout.index];
        let linked_war_id = out
            .wars
            .iter()
            .find(|war| war.attacker_polity_id == polity_id || war.defender_polity_id == polity_id)
            .map(|war| war.id);
        let Some(war_id) = linked_war_id else {
            continue;
        };
        for demand in settlement.recent_military_demands.iter().take(3) {
            let target_territory_id = territory_id_by_key
                .get(&(layout.index, demand.target_territory_key.clone()))
                .copied();
            out.military_demands.push(MilitaryDemand {
                id: out.next_military_demand_id,
                war_id,
                polity_id,
                stage: demand.stage,
                required: demand.required.clone(),
                delivered: Vec::new(),
                cash_required: demand.cash_required,
                cash_delivered: 0,
                target_territory_id,
                priority: 70,
                deadline_day: 2,
                status: MilitaryDemandStatus::Open,
                shortage_score: demand.shortage_score,
                created_day: 1,
            });
            out.next_military_demand_id += 1;
        }
    }

    for layout in village_layouts {
        let Some(settlement) = history.settlements.get(layout.index) else {
            continue;
        };
        let actor_id = settlement
            .leader_person_id
            .and_then(|id| old_to_new_id.get(&id).copied())
            .or_else(|| {
                selected_agent_seeds
                    .iter()
                    .find(|seed| seed.settlement_index == layout.index)
                    .map(|seed| seed.new_id)
            })
            .unwrap_or(0);
        for duty in settlement.recent_feudal_duties.iter().take(3) {
            out.events.push(WorldEvent {
                day: 1,
                tick: 0,
                actor: actor_id,
                target: representative_agent_by_household
                    .get(&(layout.index, duty.household_id))
                    .copied(),
                kind: match (duty.kind, duty.complied) {
                    (crate::world_history::HistoricalFeudalDutyKind::Tribute, true) => {
                        EventKind::TributePaid
                    }
                    (crate::world_history::HistoricalFeudalDutyKind::Tribute, false) => {
                        EventKind::TributeRefused
                    }
                    (crate::world_history::HistoricalFeudalDutyKind::Levy, true) => {
                        EventKind::LevyCalled
                    }
                    (crate::world_history::HistoricalFeudalDutyKind::Levy, false) => {
                        EventKind::LevyRefused
                    }
                    (crate::world_history::HistoricalFeudalDutyKind::Corvee, _) => {
                        EventKind::FeudalSanction
                    }
                },
                summary: format!(
                    "{} | {} Montante: {}.",
                    layout.name, duty.summary, duty.amount
                ),
                impact_tags: vec!["pre_historia".to_string(), "dever_feudal".to_string()],
            });
        }
        for construction in settlement.recent_constructions.iter().take(2) {
            out.events.push(WorldEvent {
                day: 1,
                tick: 0,
                actor: actor_id,
                target: None,
                kind: EventKind::Construction,
                summary: format!(
                    "{} | {} Territorio: {}.",
                    layout.name, construction.summary, construction.target_territory_key
                ),
                impact_tags: vec![
                    "pre_historia".to_string(),
                    construction.establishment_type_id.clone(),
                ],
            });
        }
        for justice in settlement.recent_justice_cases.iter().take(2) {
            out.events.push(WorldEvent {
                day: 1,
                tick: 0,
                actor: actor_id,
                target: justice.victim_household_id.and_then(|household_id| {
                    representative_agent_by_household
                        .get(&(layout.index, household_id))
                        .copied()
                }),
                kind: if justice.punitive {
                    EventKind::Punishment
                } else {
                    EventKind::Investigation
                },
                summary: format!("{} | {}", layout.name, justice.summary),
                impact_tags: vec!["pre_historia".to_string(), "justica".to_string()],
            });
        }
        for demand in settlement.recent_military_demands.iter().take(2) {
            out.events.push(WorldEvent {
                day: 1,
                tick: 0,
                actor: actor_id,
                target: None,
                kind: EventKind::MilitarySupply,
                summary: format!("{} | {}", layout.name, demand.summary),
                impact_tags: vec!["pre_historia".to_string(), "guerra".to_string()],
            });
        }
        if let Some(insurrection) = &settlement.recent_insurrection {
            out.events.push(WorldEvent {
                day: 1,
                tick: 0,
                actor: actor_id,
                target: None,
                kind: EventKind::FactionShift,
                summary: insurrection.summary.clone(),
                impact_tags: vec!["pre_historia".to_string(), insurrection.agenda_tag.clone()],
            });
        }
        for pressure in settlement.recent_pressures.iter().take(2) {
            out.events.push(WorldEvent {
                day: 1,
                tick: 0,
                actor: actor_id,
                target: None,
                kind: EventKind::PoliticalPressure,
                summary: format!("{} | {}", layout.name, pressure.reason),
                impact_tags: vec!["pre_historia".to_string(), pressure.agenda_tag.clone()],
            });
        }
        for event in settlement.ledger.iter().rev().take(3).rev() {
            out.events.push(WorldEvent {
                day: 1,
                tick: 0,
                actor: actor_id,
                target: None,
                kind: historical_event_kind_to_runtime(event.kind, &event.tags),
                summary: format!("{} | ano {} | {}", layout.name, event.year, event.summary),
                impact_tags: {
                    let mut tags = event.tags.clone();
                    tags.push(format!("importancia_{}", event.importance));
                    tags
                },
            });
        }
        if let Some(succession) = &settlement.recent_succession {
            out.events.push(WorldEvent {
                day: 1,
                tick: 0,
                actor: actor_id,
                target: None,
                kind: EventKind::SuccessionContested,
                summary: succession.summary.clone(),
                impact_tags: vec!["sucessao".to_string(), "heranca".to_string()],
            });
        }
    }
    for war in &history.wars {
        let actor_id = history
            .settlements
            .get(war.attacker_settlement_id)
            .and_then(|settlement| settlement.leader_person_id)
            .and_then(|id| old_to_new_id.get(&id).copied())
            .unwrap_or(0);
        out.events.push(WorldEvent {
            day: 1,
            tick: 0,
            actor: actor_id,
            target: None,
            kind: EventKind::InstitutionalDispute,
            summary: format!("{} Inicio: ano {}.", war.summary, war.started_year),
            impact_tags: vec!["guerra".to_string(), "pre_historia".to_string()],
        });
    }

    out
}

fn historical_person(settlement: &HistoricalSettlement, person_id: u64) -> &HistoricalPerson {
    settlement
        .people
        .iter()
        .find(|person| person.id == person_id)
        .expect("historical person missing")
}

fn historical_age(person: &HistoricalPerson, current_year: i32) -> i32 {
    (current_year - person.birth_year).max(0)
}

fn historical_household_score(household: &HistoricalHousehold) -> i32 {
    household.wealth + household.grain + household.social_rank * 2 + household.legitimacy
        - household.hardship * 3
        - household.feudal_arrears * 4
}

fn select_role_candidate(
    settlement: &HistoricalSettlement,
    current_year: i32,
    role_id: &str,
    preferred_household_id: Option<u64>,
    selected_household_ids: &HashSet<u64>,
    used_people: &HashSet<u64>,
) -> Option<u64> {
    let score_person = |person: &HistoricalPerson| {
        let household = settlement
            .households
            .iter()
            .find(|household| household.id == person.household_id)
            .expect("historical household missing");
        let mut score = match role_id {
            "lider_local" => person.leadership * 2 + person.legitimacy + household.social_rank,
            "guarda" => person.martial * 2 + person.leadership + person.legitimacy / 2,
            "ferreiro" => person.craft * 2 + person.diligence + household.wealth / 4,
            "padeiro" => person.craft + person.diligence * 2 + person.sociability / 2,
            "taverneiro" => person.sociability * 2 + person.craft + person.legitimacy / 2,
            "campones" => person.diligence * 2 + person.martial / 2 + household.grain,
            _ => person.diligence + person.sociability + person.legitimacy,
        };
        if preferred_household_id == Some(person.household_id) {
            score += 16;
        }
        if selected_household_ids.contains(&person.household_id) {
            score += 12;
        }
        score - person.trauma / 3
    };

    let pick_from = |restrict_to_selected: bool| {
        settlement
            .people
            .iter()
            .filter(|person| {
                person.alive
                    && historical_age(person, current_year) >= 16
                    && !used_people.contains(&person.id)
                    && (!restrict_to_selected
                        || selected_household_ids.contains(&person.household_id))
            })
            .max_by_key(|person| score_person(person))
            .map(|person| person.id)
    };

    pick_from(true).or_else(|| pick_from(false))
}

fn available_beds_by_home(spatial: &SpatialSnapshot) -> HashMap<BuildingId, Vec<TileCoord>> {
    let mut map: HashMap<BuildingId, Vec<TileCoord>> = HashMap::new();
    for fixture in spatial
        .fixtures
        .iter()
        .filter(|fixture| fixture.kind == FixtureKind::Bed)
    {
        if let Some(building_id) = fixture.building_id {
            map.entry(building_id).or_default().push(fixture.coord);
        }
    }
    for beds in map.values_mut() {
        beds.sort_by_key(|coord| (coord.y, coord.x));
    }
    map
}

fn building_entrance(spatial: &SpatialSnapshot, building_id: BuildingId) -> TileCoord {
    spatial
        .buildings
        .iter()
        .find(|building| building.id == building_id)
        .map(|building| building.entrance)
        .unwrap_or_default()
}

fn building_name(spatial: &SpatialSnapshot, building_id: BuildingId) -> String {
    spatial
        .buildings
        .iter()
        .find(|building| building.id == building_id)
        .map(|building| building.name.clone())
        .unwrap_or_else(|| format!("Predio {}", building_id))
}

fn room_at_position(spatial: &SpatialSnapshot, position: TileCoord) -> Option<RoomId> {
    spatial
        .grid
        .tiles
        .iter()
        .find(|tile| tile.coord == position)
        .and_then(|tile| tile.room_id)
}

fn first_storage_fixture(spatial: &SpatialSnapshot, building_id: BuildingId) -> Option<FixtureId> {
    spatial
        .fixtures
        .iter()
        .find(|fixture| {
            fixture.building_id == Some(building_id) && fixture.kind == FixtureKind::Storage
        })
        .map(|fixture| fixture.id)
}

fn establishment_def<'a>(
    catalog: &'a EconomyCatalog,
    establishment_type_id: &str,
) -> Option<&'a EstablishmentTypeDef> {
    catalog
        .establishment_types
        .iter()
        .find(|def| def.id == establishment_type_id)
}

fn owner_households_for_establishment(
    settlement_index: usize,
    establishment_type_id: &str,
    role_home_ids: &HashMap<(usize, String), Vec<BuildingId>>,
) -> Vec<BuildingId> {
    match establishment_type_id {
        "forja" => role_home_ids
            .get(&(settlement_index, "ferreiro".to_string()))
            .cloned()
            .unwrap_or_default(),
        "padaria" => role_home_ids
            .get(&(settlement_index, "padeiro".to_string()))
            .cloned()
            .unwrap_or_default(),
        "taverna" => role_home_ids
            .get(&(settlement_index, "taverneiro".to_string()))
            .cloned()
            .unwrap_or_default(),
        "fazenda" | "lenhal" | "pedreira" => role_home_ids
            .get(&(settlement_index, "campones".to_string()))
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn initial_household_pantry(household: &HistoricalHousehold) -> Vec<ResourceStack> {
    let mut pantry = Vec::new();
    if household.grain > 0 {
        pantry.push(ResourceStack {
            resource_id: "graos".to_string(),
            amount: (household.grain / 2).clamp(2, 18),
        });
    }
    if household.wealth >= 60 {
        pantry.push(ResourceStack {
            resource_id: "pao".to_string(),
            amount: 2,
        });
    }
    pantry
}

fn initial_establishment_stock(
    def: &EstablishmentTypeDef,
    settlement: &HistoricalSettlement,
) -> Vec<ResourceStack> {
    let total_grain = settlement
        .households
        .iter()
        .map(|household| household.grain)
        .sum::<i32>();
    let total_wood = settlement
        .households
        .iter()
        .map(|household| household.wood)
        .sum::<i32>();
    let total_ore = settlement
        .households
        .iter()
        .map(|household| household.ore)
        .sum::<i32>();
    let mut stock = def.default_stock.clone();
    match def.id.as_str() {
        "fazenda" => push_or_add_resource(&mut stock, "graos", (total_grain / 2).clamp(6, 26)),
        "lenhal" => {
            push_or_add_resource(&mut stock, "lenha", (total_wood / 2).clamp(4, 18));
            push_or_add_resource(&mut stock, "madeira", (total_wood / 3).clamp(2, 10));
        }
        "pedreira" => {
            push_or_add_resource(&mut stock, "metal_bruto", (total_ore / 2).clamp(3, 14));
            push_or_add_resource(&mut stock, "pedra", (total_ore / 3).clamp(2, 10));
        }
        "forja" => {
            push_or_add_resource(&mut stock, "metal_bruto", (total_ore / 4).clamp(2, 8));
            push_or_add_resource(&mut stock, "lenha", (total_wood / 4).clamp(2, 7));
        }
        "padaria" => {
            push_or_add_resource(&mut stock, "graos", (total_grain / 4).clamp(3, 12));
            push_or_add_resource(&mut stock, "lenha", (total_wood / 5).clamp(2, 6));
            push_or_add_resource(&mut stock, "pao", (total_grain / 8).clamp(0, 8));
        }
        "taverna" => {
            push_or_add_resource(&mut stock, "graos", (total_grain / 5).clamp(2, 10));
            push_or_add_resource(&mut stock, "lenha", (total_wood / 5).clamp(2, 6));
            push_or_add_resource(&mut stock, "caldo", (total_grain / 10).clamp(0, 6));
        }
        _ => {}
    }
    stock
}

fn initial_establishment_cash(
    def: &EstablishmentTypeDef,
    settlement: &HistoricalSettlement,
) -> i32 {
    match def.id.as_str() {
        "solar" | "posto_guarda" => (settlement.polity.treasury / 3).clamp(20, 120),
        "forja" => (settlement.polity.treasury / 6).clamp(12, 60),
        "padaria" | "taverna" => (settlement.polity.treasury / 7).clamp(8, 45),
        _ => (settlement.polity.treasury / 8).clamp(6, 36),
    }
}

fn build_posted_prices(
    catalog: &EconomyCatalog,
    stock_targets: &[ResourceStack],
    stock: &[ResourceStack],
) -> Vec<PostedPrice> {
    let mut seen = HashSet::new();
    let mut prices = Vec::new();
    for stack in stock_targets.iter().chain(stock.iter()) {
        if seen.insert(stack.resource_id.clone()) {
            let price = catalog
                .resources
                .iter()
                .find(|resource| resource.id == stack.resource_id)
                .map(|resource| resource.base_price)
                .unwrap_or(1);
            prices.push(PostedPrice {
                resource_id: stack.resource_id.clone(),
                unit_price: price,
            });
        }
    }
    prices
}

fn build_scarcity_metrics(
    establishments: &[EstablishmentEconomy],
    households: &[HouseholdEconomy],
) -> Vec<ScarcityMetric> {
    let total_grains = establishments
        .iter()
        .flat_map(|establishment| establishment.stock.iter())
        .filter(|stack| stack.resource_id == "graos")
        .map(|stack| stack.amount)
        .sum::<i32>()
        + households
            .iter()
            .flat_map(|household| household.pantry.iter())
            .filter(|stack| stack.resource_id == "graos")
            .map(|stack| stack.amount)
            .sum::<i32>();
    let total_bread = establishments
        .iter()
        .flat_map(|establishment| establishment.stock.iter())
        .filter(|stack| stack.resource_id == "pao")
        .map(|stack| stack.amount)
        .sum::<i32>();
    let total_broth = establishments
        .iter()
        .flat_map(|establishment| establishment.stock.iter())
        .filter(|stack| stack.resource_id == "caldo")
        .map(|stack| stack.amount)
        .sum::<i32>();
    vec![
        ScarcityMetric {
            resource_id: "graos".to_string(),
            pressure: (36 - total_grains).max(0),
        },
        ScarcityMetric {
            resource_id: "pao".to_string(),
            pressure: (18 - total_bread).max(0),
        },
        ScarcityMetric {
            resource_id: "caldo".to_string(),
            pressure: (16 - total_broth).max(0),
        },
    ]
}

fn build_profile(
    person: &HistoricalPerson,
    household: &HistoricalHousehold,
    role_id: &str,
) -> AgentProfile {
    AgentProfile {
        traits: person.traits.clone(),
        values: person.values.clone(),
        fears: person.fears.clone(),
        long_term_desires: role_desires(role_id, household),
        moral_tolerances: role_moral_tolerances(role_id, person),
        social_style: role_social_style(role_id, person),
        trauma_traits: if person.trauma + household.trauma_memory + household.war_exposure >= 25 {
            vec!["carrega trauma historico e familiar".to_string()]
        } else {
            Vec::new()
        },
    }
}

fn build_agent_state(
    person: &HistoricalPerson,
    household: &HistoricalHousehold,
    role_id: &str,
) -> AgentState {
    AgentState {
        mood: (50 + person.sociability / 6 - household.rage / 4 + household.prestige / 20)
            .clamp(0, 100),
        energy: (55 + person.diligence / 4 - (person.trauma + household.trauma_memory) / 8)
            .clamp(20, 100),
        health: (75 - person.trauma / 4 - household.war_exposure / 10).clamp(35, 100),
        hunger: (10 + household.hardship * 6).clamp(0, 100),
        stress: (15 + household.hardship * 5 + person.trauma / 2 + household.war_exposure / 3)
            .clamp(0, 100),
        current_focus: role_focus(role_id).to_string(),
        active_goals: vec![derive_long_term_plan(role_id, household, None)],
    }
}

fn initial_historical_injury(
    person: &HistoricalPerson,
    household: &HistoricalHousehold,
) -> InjuryState {
    let mut injury = InjuryState::default();
    let historical_damage =
        person.trauma / 4 + household.war_exposure / 3 + household.trauma_memory / 5;
    if historical_damage >= 12 {
        injury.pain = historical_damage.clamp(1, 45);
        injury.recovery_ticks = (historical_damage as u32 * 6).clamp(12, 240);
    }
    if household.war_exposure >= 35 {
        injury.bleeding = (household.war_exposure / 18).clamp(0, 5);
    }
    injury
}

fn build_institutional_perception(
    person: &HistoricalPerson,
    household: &HistoricalHousehold,
    role_id: &str,
    settlement: &HistoricalSettlement,
) -> InstitutionalPerception {
    let mut perception = InstitutionalPerception {
        leader_legitimacy: (household.legitimacy + if role_id == "lider_local" { 20 } else { 0 })
            .clamp(-100, 100),
        justice_legitimacy: (household.legitimacy - household.rage / 2).clamp(-100, 100),
        tax_legitimacy: (20 - household.feudal_arrears * 10 - household.hardship * 4)
            .clamp(-100, 100),
        rationing_legitimacy: (12 - household.hardship * 5).clamp(-100, 100),
        guard_trust: if role_id == "guarda" {
            45
        } else {
            (household.legitimacy - household.rage).clamp(-100, 100)
        },
        war_support: settlement
            .story_seeds
            .iter()
            .filter(|story| matches!(story.kind, CulturalStoryKind::CantoDeGuerra))
            .count() as i32
            * 12
            - household.hardship * 4
            - household.war_exposure / 2,
        fear_of_authority: (10
            + household.social_rank / 3
            + household.rage / 2
            + household.trauma_memory / 3)
            .clamp(-100, 100),
        perceived_corruption: (household.rage * 2 + household.feudal_arrears * 6
            - household.legitimacy)
            .clamp(-100, 100),
        perceived_fairness: (household.legitimacy - household.hardship * 4).clamp(-100, 100),
        last_updated_day: 1,
        notes: vec![format!("Moldado pela pre-historia de {}", settlement.name)],
    };
    if person.trauma >= 30 {
        perception.fear_of_authority = (perception.fear_of_authority + 15).clamp(-100, 100);
    }
    perception.clamp_all();
    perception
}

fn build_psychology(
    person: &HistoricalPerson,
    household: &HistoricalHousehold,
    role_id: &str,
    settlement_name: Option<&str>,
    day: u32,
) -> PsychologicalState {
    let mut state = PsychologicalState {
        grief: ((person.trauma + household.trauma_memory) / 3).clamp(0, 100),
        humiliation: (household.hardship * 4 + household.feudal_arrears * 3).clamp(0, 100),
        fear: (person.trauma / 2 + household.rage + household.war_exposure / 2).clamp(0, 100),
        pride: (household.social_rank + household.prestige / 2 + person.legitimacy / 2)
            .clamp(0, 100),
        trauma: (person.trauma + household.trauma_memory + household.war_exposure / 2)
            .clamp(0, 100),
        anger: (household.rage * 2 + household.feudal_arrears).clamp(0, 100),
        hope: (45 + person.diligence / 3 + household.legitimacy / 5 - household.hardship * 2)
            .clamp(0, 100),
        guilt: if role_id == "lider_local" {
            household.hardship.clamp(0, 60)
        } else {
            (person.trauma / 4).clamp(0, 50)
        },
        status_anxiety: (household.hardship * 3
            + (45 - household.prestige).max(0)
            + household.feudal_arrears)
            .clamp(0, 100),
        revenge_drive: (household.rage + household.cultural_pressure / 2).clamp(0, 100),
        submission_drive: ((person.trauma + household.trauma_memory) / 3 + household.hardship * 2)
            .clamp(0, 100),
        dominance_drive: (household.social_rank + household.prestige / 2 + person.legitimacy / 3)
            .clamp(0, 100),
        last_public_humiliation_tick: 0,
        last_public_humiliation_by: None,
        active_revenge_target: None,
        long_term_plan: String::new(),
        personal_symbols: Vec::new(),
        coping_patterns: Vec::new(),
        inner_contradictions: Vec::new(),
        melancholic_fixation: None,
        last_updated_day: day,
        notes: vec![format!("Herdou marcas de {}", role_focus(role_id))],
    };
    state.long_term_plan = derive_long_term_plan(role_id, household, settlement_name);
    if household.trauma_memory >= 20 || person.trauma >= 30 {
        state.personal_symbols.push(PersonalSymbol {
            target_kind: PersonalSymbolTargetKind::Event,
            target_id: None,
            text: "memoria herdada de perda".to_string(),
            meaning: "a historia da casa ainda pede cuidado".to_string(),
            emotion: "melancolia".to_string(),
            intensity: (household.trauma_memory + person.trauma / 2).clamp(10, 65),
            origin_memory_id: None,
        });
        state.coping_patterns.push(CopingPattern {
            kind: CopingPatternKind::RitualReturn,
            trigger: "memoria familiar dolorosa".to_string(),
            behavior_hint:
                "procurar silencio, lugar seguro ou historia antiga quando a dor retorna"
                    .to_string(),
            strength: (household.trauma_memory / 2 + person.trauma / 3).clamp(8, 45),
            last_triggered_tick: 0,
        });
    }
    if household.hardship >= 8 || household.feudal_arrears >= 8 {
        state.inner_contradictions.push(InnerContradiction {
            desire: "manter dignidade familiar".to_string(),
            fear: "ser esmagado por fome, divida ou autoridade".to_string(),
            compromise: "aceitar rotina dura enquanto procura brecha de estabilidade".to_string(),
            pressure: (household.hardship * 3 + household.feudal_arrears * 2).clamp(8, 60),
        });
        state.melancholic_fixation =
            Some("a casa precisa sobreviver sem perder o nome".to_string());
    }
    state.clamp_all();
    state
}

fn build_craft_proficiencies(
    person: &HistoricalPerson,
    role_id: &str,
    household: &HistoricalHousehold,
) -> CraftProficiencyState {
    let base = (person.craft / 2 + household.social_rank / 3).clamp(0, 100);
    let mut prof = CraftProficiencyState {
        smithing: if matches!(role_id, "ferreiro" | "guarda" | "lider_local") {
            (base + 20).clamp(0, 100)
        } else {
            (base / 2).clamp(0, 100)
        },
        tailoring: if matches!(role_id, "taverneiro" | "lider_local") {
            (base + 10).clamp(0, 100)
        } else {
            (base / 2).clamp(0, 100)
        },
        jewelry: if role_id == "lider_local" {
            (base + 12).clamp(0, 100)
        } else {
            (base / 3).clamp(0, 100)
        },
        leatherwork: if matches!(role_id, "campones" | "guarda") {
            (base + 8).clamp(0, 100)
        } else {
            (base / 3).clamp(0, 100)
        },
    };
    prof.clamp_all();
    prof
}

fn seeded_equipment_resource_ids(
    role_id: &str,
    household: &HistoricalHousehold,
    person: &HistoricalPerson,
) -> Vec<&'static str> {
    let mut items = vec!["tunica_simples"];
    if household.wealth >= 55 || household.social_rank >= 30 {
        items.push("tunica_boa");
    }
    if role_id == "guarda" {
        items.push("lanca_simples");
        items.push("escudo");
        if household.wealth >= 45 {
            items.push("capacete_simples");
            items.push("gambesao");
        }
    } else if role_id == "ferreiro" {
        items.push("machado");
        if household.wealth >= 50 {
            items.push("couro_reforcado");
        }
    } else if role_id == "lider_local" {
        items.push("espada_curta");
        items.push("manto");
        items.push("anel_prata");
        if household.wealth >= 65 || person.legitimacy >= 60 {
            items.push("broche");
        }
    } else if role_id == "taverneiro" && household.wealth >= 45 {
        items.push("manto");
    } else if role_id == "padeiro" && household.wealth >= 35 {
        items.push("tunica_boa");
    } else if household.wealth >= 70 {
        items.push("anel_cobre");
    }
    items
}

fn create_seeded_item_instance(
    catalog: &EconomyCatalog,
    next_item_instance_id: &mut ItemInstanceId,
    resource_id: &str,
    owner_agent_id: u64,
    owner_household_id: Option<BuildingId>,
    maker_agent_id: Option<u64>,
    maker_name_snapshot: Option<String>,
    proficiencies: &CraftProficiencyState,
) -> Option<ItemInstance> {
    let resource = catalog
        .resources
        .iter()
        .find(|item| item.id == resource_id)?;
    let item_class = resource.item_class?;
    let proficiency = match item_class {
        ItemClass::Clothing => proficiencies.tailoring,
        ItemClass::Jewelry => proficiencies.jewelry,
        _ => proficiencies.smithing,
    };
    let quality = (38 + proficiency / 2 + (*next_item_instance_id as i32 % 9) - 4).clamp(0, 100);
    let refinement_level = RefinementLevel::from_quality_score(quality);
    let tier = refinement_level.tier();
    let mut combat_profile = resource.base_combat_stats.clone();
    combat_profile.damage += resource.refinement_scaling.damage_per_tier * tier;
    combat_profile.precision += resource.refinement_scaling.precision_per_tier * tier;
    combat_profile.protection += resource.refinement_scaling.protection_per_tier * tier;
    combat_profile.injury_severity += tier / 2;
    let item = ItemInstance {
        id: *next_item_instance_id,
        resource_id: resource.id.clone(),
        display_name: format!(
            "{} {}",
            resource.display_name,
            refinement_level.display_name()
        ),
        refinement_level,
        craft_quality_score: quality,
        durability: (resource.base_durability
            + resource.refinement_scaling.durability_per_tier * tier)
            .max(1),
        maker_agent_id,
        maker_name_snapshot,
        material_signature: resource.material_tags.join("+"),
        combat_profile,
        prestige_value: (resource.base_prestige
            + resource.refinement_scaling.prestige_per_tier * tier)
            .max(0),
        owner_agent_id: Some(owner_agent_id),
        owner_household_id,
    };
    *next_item_instance_id += 1;
    Some(item)
}

fn derive_long_term_plan(
    role_id: &str,
    household: &HistoricalHousehold,
    settlement_name: Option<&str>,
) -> String {
    let place = settlement_name.unwrap_or("a vila");
    match role_id {
        "lider_local" => format!("consolidar autoridade e manter {} abastecida", place),
        "guarda" => format!("preservar ordem e conter ameacas em {}", place),
        "ferreiro" => "manter a oficina vital e acumular ferramentas".to_string(),
        "padeiro" => "garantir graos e sustentar a producao de pao".to_string(),
        "taverneiro" => "manter a taverna como centro de trocas e informacao".to_string(),
        "campones" if household.hardship >= 3 => {
            "repor reservas da casa e escapar da fome futura".to_string()
        }
        "campones" => "proteger a casa e ampliar a colheita do lar".to_string(),
        _ => "manter a casa viva e ganhar margem de seguranca".to_string(),
    }
}

fn role_desires(role_id: &str, household: &HistoricalHousehold) -> Vec<String> {
    let mut desires = match role_id {
        "lider_local" => vec![
            "consolidar autoridade local".to_string(),
            "evitar colapso politico".to_string(),
        ],
        "guarda" => vec![
            "manter ordem e reputacao".to_string(),
            "evitar humilhacao publica".to_string(),
        ],
        "ferreiro" => vec![
            "fortalecer a forja".to_string(),
            "acumular recursos solidos".to_string(),
        ],
        "padeiro" => vec![
            "evitar falta de alimento".to_string(),
            "manter a padaria essencial".to_string(),
        ],
        "taverneiro" => vec![
            "preservar centralidade social".to_string(),
            "ouvir e influenciar rumores".to_string(),
        ],
        _ => vec![
            "proteger a familia".to_string(),
            "manter reserva alimentar".to_string(),
        ],
    };
    if household.feudal_arrears > 0 {
        desires.push("reduzir atrasos feudais".to_string());
    }
    desires
}

fn role_moral_tolerances(role_id: &str, person: &HistoricalPerson) -> Vec<String> {
    let mut tolerances = Vec::new();
    if matches!(role_id, "lider_local" | "guarda") {
        tolerances.push("tolera coercao quando julga a ordem em risco".to_string());
    }
    if person.trauma >= 25 {
        tolerances.push("tolera pequenos desvios por sobrevivencia".to_string());
    }
    if person.martial >= 70 {
        tolerances.push("aceita violencia defensiva".to_string());
    }
    if tolerances.is_empty() {
        tolerances.push("prefere reciprocidade prudente".to_string());
    }
    tolerances
}

fn role_social_style(role_id: &str, person: &HistoricalPerson) -> String {
    match role_id {
        "lider_local" => "autoritario e calculado".to_string(),
        "guarda" => "vigilante e formal".to_string(),
        "ferreiro" => "direto e pragmatico".to_string(),
        "padeiro" => "prestativo e atento".to_string(),
        "taverneiro" => "gregario e observador".to_string(),
        _ if person.sociability >= 60 => "comunitario e adaptavel".to_string(),
        _ => "reservado e cauteloso".to_string(),
    }
}

fn role_focus(role_id: &str) -> &'static str {
    match role_id {
        "lider_local" => "governo local",
        "guarda" => "patrulha e coercao",
        "ferreiro" => "oficina",
        "padeiro" => "cadeia alimentar",
        "taverneiro" => "troca social",
        "campones" => "campo e abastecimento",
        _ => "sobrevivencia domestica",
    }
}

fn story_moral_hint(role_id: &str) -> String {
    match role_id {
        "lider_local" => "ordem exige memoria politica".to_string(),
        "guarda" => "seguranca depende de disciplina".to_string(),
        "campones" => "fome ensina prudencia coletiva".to_string(),
        _ => "a historia orienta sobrevivencia e reputacao".to_string(),
    }
}

fn populate_relations(agents: &mut [AgentSnapshot]) {
    let len = agents.len();
    for i in 0..len {
        for j in 0..len {
            if i == j {
                continue;
            }
            let mut relation = AgentRelation::default();
            if agents[i].home_building_id == agents[j].home_building_id {
                relation.trust += 28;
                relation.friendship += 18;
            }
            if agents[i].spouse == Some(agents[j].id) {
                relation.trust += 30;
                relation.friendship += 25;
                relation.attraction += 20;
            }
            if agents[i].parents.contains(&agents[j].id)
                || agents[i].children.contains(&agents[j].id)
            {
                relation.trust += 22;
                relation.friendship += 12;
            }
            if relation.trust != 0
                || relation.friendship != 0
                || relation.attraction != 0
                || relation.resentment != 0
            {
                relation.last_updated_day = 1;
                relation
                    .notes
                    .push("Vinculo herdado do bootstrap historico.".to_string());
                agents[i].relations.insert(agents[j].id, relation);
            }
        }
    }
}

fn role_agent_id(
    settlement_index: usize,
    role_id: &str,
    selected_agent_seeds: &[SelectedAgentSeed],
) -> Option<u64> {
    selected_agent_seeds
        .iter()
        .find(|seed| seed.settlement_index == settlement_index && seed.role_id == role_id)
        .map(|seed| seed.new_id)
}

fn pressure_domain(agenda_tag: &str) -> PolicyDomain {
    match agenda_tag {
        "motim_comida" => PolicyDomain::Rationing,
        "boicote_imposto" | "imposto_guerra" => PolicyDomain::Tax,
        _ => PolicyDomain::Justice,
    }
}

fn policy_from_tag(
    tag: &str,
    layout: &VillageLayout,
    settlement: &HistoricalSettlement,
    polity_id: PolityId,
    territory_id_by_key: &HashMap<(usize, String), TerritoryId>,
) -> (PolicyScope, PolicyTarget, Vec<PolicyEffect>, String) {
    match tag {
        "imposto_guerra" => (
            PolicyScope::Polity(polity_id),
            PolicyTarget::None,
            vec![PolicyEffect::TaxRate { rate_percent: 115 }],
            format!(
                "{} elevou tributos para sustentar pressao militar.",
                layout.name
            ),
        ),
        "racionamento_estrito" => (
            PolicyScope::Territory(
                territory_id_by_key[&(layout.index, "vila_central".to_string())],
            ),
            PolicyTarget::None,
            vec![PolicyEffect::RationingRule {
                policy: settlement.local_norms.rationing_policy,
                energy_gain_percent: 90,
            }],
            format!(
                "{} entrou em racionamento mais duro para conter escassez.",
                layout.name
            ),
        ),
        _ => (
            PolicyScope::Polity(polity_id),
            PolicyTarget::None,
            Vec::new(),
            format!(
                "{} preserva um ato historico residual: {}.",
                layout.name, tag
            ),
        ),
    }
}

fn push_or_add_resource(stacks: &mut Vec<ResourceStack>, resource_id: &str, amount: i32) {
    if amount <= 0 {
        return;
    }
    if let Some(existing) = stacks
        .iter_mut()
        .find(|stack| stack.resource_id == resource_id)
    {
        existing.amount += amount;
    } else {
        stacks.push(ResourceStack {
            resource_id: resource_id.to_string(),
            amount,
        });
    }
}

fn historical_event_kind_to_runtime(kind: HistoricalEventKind, tags: &[String]) -> EventKind {
    match kind {
        HistoricalEventKind::Demography => {
            if tags.iter().any(|tag| tag == "morte") {
                EventKind::Death
            } else {
                EventKind::Routine
            }
        }
        HistoricalEventKind::Scarcity => EventKind::Scarcity,
        HistoricalEventKind::Commerce => EventKind::Commerce,
        HistoricalEventKind::Construction => EventKind::Construction,
        HistoricalEventKind::Succession => EventKind::SuccessionOpened,
        HistoricalEventKind::Decree => EventKind::NormChanged,
        HistoricalEventKind::FeudalObligation => {
            if tags.iter().any(|tag| tag == "tributo") {
                EventKind::TributeDemanded
            } else if tags.iter().any(|tag| tag == "levy") {
                EventKind::LevyCalled
            } else {
                EventKind::FeudalSanction
            }
        }
        HistoricalEventKind::CrimeAndJustice => EventKind::Punishment,
        HistoricalEventKind::FactionalConflict => EventKind::InstitutionalDispute,
        HistoricalEventKind::WarImpact => EventKind::InstitutionalDispute,
        HistoricalEventKind::CulturalTransmission => EventKind::CulturalStory,
    }
}
