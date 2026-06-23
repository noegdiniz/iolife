// Cores centralizadas do render 2D (RGB 0-255).
// Referenciadas por map_render, agent_render e camera.
// Hex values: comentarios documentais, nao usados em runtime.

// ── Fundo ──

/// Fundo preto puro da camera ClearColor e tiles fora do grid. #000000
pub const BACKGROUND: (u8, u8, u8) = (0, 0, 0);

// ── Terreno (TileKind) ──

pub const TILE_GRASS: (u8, u8, u8) = (26, 52, 26); // pasto verde escuro    #1A341A
pub const TILE_ROAD: (u8, u8, u8) = (90, 60, 30); // terra batida          #5A3C1E
pub const TILE_FLOOR: (u8, u8, u8) = (122, 106, 90); // tabua de madeira      #7A6A5A
pub const TILE_WALL: (u8, u8, u8) = (58, 58, 58); // pedra escura          #3A3A3A
pub const TILE_DOOR: (u8, u8, u8) = (138, 90, 58); // portal de madeira     #8A5A3A
pub const TILE_FIELD: (u8, u8, u8) = (106, 138, 42); // plantacao verde-ama   #6A8A2A
pub const TILE_FOREST: (u8, u8, u8) = (10, 52, 10); // mata fechada          #0A340A
pub const TILE_ROCK: (u8, u8, u8) = (90, 90, 90); // rocha exposta         #5A5A5A

// ── Papeis (role_id) ──

pub const AGENT_LEADER: (u8, u8, u8) = (217, 166, 33); // lider_local — dourado     #D9A621
pub const AGENT_GUARD: (u8, u8, u8) = (191, 38, 38); // guarda — vermelho         #BF2626
pub const AGENT_FARMER: (u8, u8, u8) = (38, 166, 38); // campones — verde          #26A626
pub const AGENT_SMITH: (u8, u8, u8) = (140, 140, 140); // ferreiro — cinza metal    #8C8C8C
pub const AGENT_BAKER: (u8, u8, u8) = (191, 140, 38); // padeiro — marrom trigo    #BF8C26
pub const AGENT_TAVERN: (u8, u8, u8) = (217, 115, 13); // taverneiro — laranja      #D9730D
pub const AGENT_OTHER: (u8, u8, u8) = (51, 115, 191); // outros papeis — azul      #3373BF

// ── Estados especiais de agente ──

pub const AGENT_DEAD: (u8, u8, u8) = (64, 20, 20); // morto — vermelho escuro     #401414
pub const AGENT_SELECTED: (u8, u8, u8) = (255, 255, 255); // selecionado — branco puro   #FFFFFF
pub const AGENT_CONVERSING: (u8, u8, u8) = (255, 204, 51); // em conversa — amarelo       #FFCC33
