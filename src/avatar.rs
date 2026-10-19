//! Avatar animato basato sulla caricatura originale `docs/matteo caricatura.jpeg`.
//!
//! La caricatura (scontornata con canale alfa anti-alias in `matteo_caricatura.png`)
//! viene incorporata nell'eseguibile e animata in tempo reale tramite un rig
//! 2D a mesh deformabile (in stile Live2D):
//! - respiro naturale del busto e delle spalle
//! - oscillazione, inclinazione e tremolio della testa attorno al perno del collo
//! - deformazione espressiva di sopracciglia, occhi, guance e bocca in base all'umore
//! - battito di palpebre periodico con palpebra e ciglia in stile fumetto
//! - gocce di sudore inchiostrate nel panico, aura dinamica e fumetto comic per i milestone.

use eframe::egui::{
    epaint::{QuadraticBezierShape, Vertex},
    Align2, Color32, ColorImage, Context, FontId, Id, Mesh, Painter, Pos2, Rect, Shape, Stroke,
    TextureHandle, TextureOptions, Vec2,
};
use std::sync::Arc;

pub const LARGHEZZA: f32 = 200.0;
pub const ALTEZZA: f32 = 230.0;

pub const VERDE: Color32 = Color32::from_rgb(46, 204, 113);
pub const ROSSO: Color32 = Color32::from_rgb(231, 76, 60);
pub const GIALLO: Color32 = Color32::from_rgb(245, 197, 24);

const INCHIOSTRO: Color32 = Color32::from_rgb(24, 14, 12);
const PELLE_PALPEBRA: Color32 = Color32::from_rgb(246, 162, 120);
const PELLE_OMBRA: Color32 = Color32::from_rgb(228, 114, 94);
const GOCCIA: Color32 = Color32::from_rgb(110, 195, 255);
const GOCCIA_SCURA: Color32 = Color32::from_rgb(65, 145, 230);

const PNG_CARICATURA: &[u8] = include_bytes!("matteo_caricatura.png");

#[derive(Debug, Clone)]
pub struct Bandiera {
    pub testo: String,
    pub colore: Color32,
    /// 0 = nascosta, 1 = fumetto visibile
    pub alzata: f32,
}

#[derive(Debug, Clone)]
pub struct StatoAvatar {
    /// -1 disperato … 0 neutro … 1 felicissimo
    pub umore: f32,
    /// Intensità dell'indicatore di anticipo (0..1)
    pub alza_verde: f32,
    /// Intensità dell'indicatore di ritardo (0..1)
    pub alza_rosso: f32,
    pub sudore: bool,
    pub applauso: bool,
    pub opacita: f32,
    pub bandiera: Option<Bandiera>,
}

impl Default for StatoAvatar {
    fn default() -> Self {
        Self {
            umore: 0.4,
            alza_verde: 0.2,
            alza_rosso: 0.2,
            sudore: false,
            applauso: false,
            opacita: 1.0,
            bandiera: None,
        }
    }
}

/// Trasforma coordinate virtuali (200 x 230) in coordinate schermo.
struct Tela<'a> {
    painter: &'a Painter,
    origine: Pos2,
    u: f32,
    opacita: f32,
}

impl Tela<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origine + Vec2::new(x, y) * self.u
    }
    fn s(&self, v: f32) -> f32 {
        v * self.u
    }
    fn col(&self, c: Color32) -> Color32 {
        if self.opacita >= 0.999 {
            c
        } else {
            c.linear_multiply(self.opacita)
        }
    }
    fn cerchio(&self, x: f32, y: f32, r: f32, c: Color32) {
        self.painter
            .circle_filled(self.p(x, y), self.s(r), self.col(c));
    }
    fn linea(&self, a: (f32, f32), b: (f32, f32), w: f32, c: Color32) {
        self.painter.line_segment(
            [self.p(a.0, a.1), self.p(b.0, b.1)],
            Stroke::new(self.s(w), self.col(c)),
        );
    }
    fn curva3(&self, p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), w: f32, c: Color32) {
        self.painter.add(QuadraticBezierShape::from_points_stroke(
            [self.p(p0.0, p0.1), self.p(p1.0, p1.1), self.p(p2.0, p2.1)],
            false,
            Color32::TRANSPARENT,
            Stroke::new(self.s(w), self.col(c)),
        ));
    }
}

/// Carica la texture della caricatura una sola volta e la conserva nella cache di egui.
fn texture_caricatura(ctx: &Context) -> TextureHandle {
    let id = Id::unique("tex_matteo_caricatura");
    if let Some(tex) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return tex;
    }
    let img = image::load_from_memory_with_format(PNG_CARICATURA, image::ImageFormat::Png)
        .expect("matteo_caricatura.png valido")
        .into_rgba8();
    let dim = [img.width() as usize, img.height() as usize];
    let color_img = ColorImage::from_rgba_unmultiplied(dim, img.as_raw());
    let tex = ctx.load_texture(
        "matteo_caricatura",
        Arc::new(color_img),
        TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

/// Influenza gaussiana morbida centrata in `(cu, cv)` con raggi `(ru, rv)`.
#[inline]
fn peso_gauss(u: f32, v: f32, cu: f32, cv: f32, ru: f32, rv: f32) -> f32 {
    let du = (u - cu) / ru;
    let dv = (v - cv) / rv;
    let d2 = du * du + dv * dv;
    if d2 > 4.5 {
        0.0
    } else {
        (-d2).exp()
    }
}

/// Disegna la caricatura animata dentro `rect` (mantiene le proporzioni, centrata).
/// `t` è il tempo in secondi, usato per le animazioni continue.
pub fn disegna(painter: &Painter, rect: Rect, stato: &StatoAvatar, t: f32) {
    let u_scala = (rect.width() / LARGHEZZA).min(rect.height() / ALTEZZA);
    let origine = rect.center() - Vec2::new(LARGHEZZA, ALTEZZA) * u_scala * 0.5;
    let tela = Tela {
        painter,
        origine,
        u: u_scala,
        opacita: stato.opacita.clamp(0.15, 1.0),
    };

    let tex = texture_caricatura(painter.ctx());
    let umore = stato.umore.clamp(-1.0, 1.0);
    let triste = (-umore).max(0.0);
    let felice = (umore - 0.35).max(0.0) / 0.65;

    // 1) Aura dinamica dietro la caricatura per colpo d'occhio immediato dal palco
    disegna_aura(&tela, stato, t);

    // 2) Calcolo parametri di animazione del corpo e della testa
    let freq_respiro = if stato.sudore {
        4.2
    } else if stato.applauso {
        5.5
    } else {
        2.1
    };
    let respiro = (t * freq_respiro).sin();

    // Oscillazione e inclinazione della testa (in radianti)
    let dondolio = if stato.sudore {
        // Tremolio rapido nel panico + testa inclinata preoccupata
        -0.045 + (t * 22.0).sin() * 0.018 + (t * 3.2).cos() * 0.020
    } else if stato.applauso {
        // Dondolio festoso a tempo con l'applauso
        (t * 7.5).sin() * 0.048
    } else if umore < -0.1 {
        // Preoccupato: inclinazione laterale e leggero abbassamento
        -0.035 * triste + (t * 2.0).sin() * 0.016
    } else if stato.alza_verde > 0.4 {
        // In anticipo: dondolio allegro e fiero
        (t * 2.6).sin() * 0.038
    } else {
        // In linea: movimento morbido e naturale
        (t * 1.6).sin() * 0.022
    };

    // Battito di palpebre (ogni ~4.1s, più un doppio battito occasionale)
    let ciclo_blink = t % 4.1;
    let chiusura_occhi = if ciclo_blink < 0.14 {
        (ciclo_blink / 0.14 * std::f32::consts::PI).sin()
    } else if (t % 11.5) > 0.22 && (t % 11.5) < 0.35 {
        (((t % 11.5) - 0.22) / 0.13 * std::f32::consts::PI).sin()
    } else {
        0.0
    };

    // Sguardo che si muove leggermente
    let sguardo_u = (t * 0.7).sin() * 0.003;
    let sguardo_v = (t * 0.5).cos() * 0.002;

    // Quando c'è il fumetto in alto, scaliamo e abbassiamo proporzionalmente il ritratto
    // così il fumetto sta sopra la testa senza mai sovrapporsi né schiacciare le proporzioni.
    let f_fumetto = stato
        .bandiera
        .as_ref()
        .map(|b| b.alzata.clamp(0.0, 1.0))
        .unwrap_or(0.0);

    let y0 = 6.0 + f_fumetto * 29.0;
    let alt_ritratto = 222.0 - f_fumetto * 29.0;
    let larg_ritratto = 154.0 * (alt_ritratto / 222.0);
    let x0 = 100.0 - larg_ritratto * 0.5;

    // Funzione che trasforma una coordinata UV (0..1, 0..1) della caricatura
    // nella posizione deformata (x, y) nello spazio virtuale 200 x 230.
    let deforma_uv = |u: f32, v: f32| -> (f32, f32) {
        let mut du = 0.0_f32;
        let mut dv = 0.0_f32;

        // --- A. Espressione Sopracciglia ---
        // Sopracciglio sinistro: interno (0.485, 0.200), arco (0.400, 0.185), esterno (0.330, 0.220)
        // Sopracciglio destro: interno (0.600, 0.205), arco (0.680, 0.195), esterno (0.745, 0.235)
        let w_sop_int_sx = peso_gauss(u, v, 0.485, 0.200, 0.065, 0.042);
        let w_sop_int_dx = peso_gauss(u, v, 0.600, 0.205, 0.065, 0.042);
        let w_sop_arc_sx = peso_gauss(u, v, 0.400, 0.185, 0.075, 0.042);
        let w_sop_arc_dx = peso_gauss(u, v, 0.680, 0.195, 0.075, 0.042);
        let w_sop_est_sx = peso_gauss(u, v, 0.330, 0.220, 0.060, 0.042);
        let w_sop_est_dx = peso_gauss(u, v, 0.745, 0.235, 0.060, 0.042);

        if triste > 0.0 {
            // Preoccupato / Panico: punte interne in alto e aggrottate, code esterne in basso
            dv -= (w_sop_int_sx + w_sop_int_dx) * 0.025 * triste;
            du += w_sop_int_sx * 0.010 * triste - w_sop_int_dx * 0.010 * triste;
            dv += (w_sop_est_sx + w_sop_est_dx) * 0.022 * triste;
        }
        if felice > 0.0 {
            // Felice / Anticipo: sopracciglia sollevate con entusiasmo
            dv -= (w_sop_arc_sx + w_sop_arc_dx) * 0.018 * felice;
            dv -= (w_sop_int_sx + w_sop_int_dx) * 0.010 * felice;
        }

        // --- B. Espressione Occhi (battito palpebre, panico sgranato, sorriso socchiuso) ---
        let w_palp_sup_sx = peso_gauss(u, v, 0.415, 0.235, 0.065, 0.024);
        let w_palp_inf_sx = peso_gauss(u, v, 0.415, 0.266, 0.065, 0.024);
        let w_palp_sup_dx = peso_gauss(u, v, 0.655, 0.242, 0.065, 0.024);
        let w_palp_inf_dx = peso_gauss(u, v, 0.655, 0.272, 0.065, 0.024);

        // Battito di palpebre
        dv += (w_palp_sup_sx + w_palp_sup_dx) * 0.028 * chiusura_occhi;
        dv -= (w_palp_inf_sx + w_palp_inf_dx) * 0.008 * chiusura_occhi;

        // Nel panico gli occhi si sgranano
        if stato.sudore || triste > 0.5 {
            let p = ((triste - 0.3).max(0.0) / 0.7).max(if stato.sudore { 0.8 } else { 0.0 });
            dv -= (w_palp_sup_sx + w_palp_sup_dx) * 0.013 * p * (1.0 - chiusura_occhi);
            dv += (w_palp_inf_sx + w_palp_inf_dx) * 0.009 * p * (1.0 - chiusura_occhi);
        } else if felice > 0.2 {
            // Quando sorride molto le palpebre inferiori salgono leggermente ("sorriso con gli occhi")
            dv -= (w_palp_inf_sx + w_palp_inf_dx) * 0.009 * felice;
        }

        // Piccolo movimento dello sguardo (iride)
        let w_iride = peso_gauss(u, v, 0.415, 0.252, 0.032, 0.016)
            + peso_gauss(u, v, 0.655, 0.258, 0.032, 0.016);
        du += w_iride * sguardo_u;
        dv += w_iride * sguardo_v;

        // --- C. Espressione Bocca, Guance e Mento ---
        // Angolo bocca sx (0.395, 0.430), dx (0.672, 0.436), centro bocca (0.540, 0.452)
        let w_bocca_sx = peso_gauss(u, v, 0.395, 0.430, 0.068, 0.052);
        let w_bocca_dx = peso_gauss(u, v, 0.672, 0.436, 0.068, 0.052);
        let w_bocca_c = peso_gauss(u, v, 0.540, 0.455, 0.085, 0.050);
        let w_guancia_sx = peso_gauss(u, v, 0.350, 0.395, 0.075, 0.060);
        let w_guancia_dx = peso_gauss(u, v, 0.715, 0.405, 0.075, 0.060);
        let w_mento = peso_gauss(u, v, 0.535, 0.545, 0.095, 0.055);

        if umore >= 0.35 {
            // Più felice della posa base: sorriso ancora più ampio e guance alte
            dv -= (w_bocca_sx + w_bocca_dx) * 0.020 * felice;
            du -= w_bocca_sx * 0.010 * felice;
            du += w_bocca_dx * 0.010 * felice;
            dv -= (w_guancia_sx + w_guancia_dx) * 0.014 * felice;
            dv += w_bocca_c * 0.010 * felice;
            dv += w_mento * 0.008 * felice;
        } else {
            // Transizione fluida dal sorriso della foto -> neutro (0.0) -> broncio preoccupato (-1.0)
            // k = 0.0 a umore 0.35, k = 0.26 a umore 0.0, k = 1.0 a umore -1.0
            let k = ((0.35 - umore) / 1.35).clamp(0.0, 1.0);
            // Gli angoli della bocca scendono e si stringono leggermente
            dv += (w_bocca_sx + w_bocca_dx) * 0.048 * k;
            du += w_bocca_sx * 0.008 * k - w_bocca_dx * 0.008 * k;
            // Le guance seguono gli angoli della bocca verso il basso
            dv += (w_guancia_sx + w_guancia_dx) * 0.022 * k;
            // Il centro delle labbra sale creando l'arco all'ingiù tipico dell'espressione preoccupata
            dv -= w_bocca_c * 0.026 * k;
            dv -= w_mento * 0.010 * k;
        }

        let u_def = u + du;
        let v_def = v + dv;

        // Coordinate nello spazio virtuale prima della rotazione della testa
        let mut x = x0 + u_def * larg_ritratto;
        let mut y = y0 + v_def * alt_ritratto;

        // --- D. Rotazione elastica della testa attorno al collo + Respiro del busto ---
        // Peso testa: 1.0 sul cranio/viso (v < 0.52), sfuma a 0.0 sul collo/colletto (v = 0.52..0.63)
        let w_testa = ((0.62 - v) / 0.11).clamp(0.0, 1.0);
        let w_testa = w_testa * w_testa * (3.0 - 2.0 * w_testa); // smoothstep

        if w_testa > 0.0 {
            let perno_x = x0 + 0.51 * larg_ritratto;
            let perno_y = y0 + 0.58 * alt_ritratto;
            let ang = dondolio * w_testa;
            let (s_a, c_a) = ang.sin_cos();
            let rx = x - perno_x;
            let ry = y - perno_y;
            x = perno_x + rx * c_a - ry * s_a;
            y = perno_y + rx * s_a + ry * c_a;
            // Leggero abbassamento della testa quando è in ritardo, o sollevamento quando è in anticipo
            y += (-umore * 2.2 + respiro * 1.1) * w_testa;
        }

        // Il busto e le spalle respirano espandendosi leggermente
        let w_busto = ((v - 0.56) / 0.20).clamp(0.0, 1.0);
        if w_busto > 0.0 {
            let centro_busto_x = x0 + 0.50 * larg_ritratto;
            x += (x - centro_busto_x) * (respiro * 0.014) * w_busto;
            y += respiro * 1.2 * (1.0 - (v - 0.60).max(0.0));
        }

        (x, y)
    };

    // 3) Costruzione e disegno della Mesh 2D ad alta densità (40 x 52 = 2080 celle)
    const COLONNE: usize = 40;
    const RIGHE: usize = 52;
    let mut mesh = Mesh::with_texture(tex.id());
    mesh.vertices.reserve((COLONNE + 1) * (RIGHE + 1));
    mesh.indices.reserve(COLONNE * RIGHE * 6);

    for iy in 0..=RIGHE {
        let v = iy as f32 / RIGHE as f32;
        for ix in 0..=COLONNE {
            let u = ix as f32 / COLONNE as f32;
            let (vx, vy) = deforma_uv(u, v);

            // Nel panico la fronte si arrossa leggermente per lo sforzo
            let mut tinta = Color32::WHITE;
            if stato.sudore && v < 0.35 {
                let rossore = ((0.35 - v) / 0.35 * 0.18).clamp(0.0, 0.18);
                let g_b = (255.0 * (1.0 - rossore)) as u8;
                tinta = Color32::from_rgb(255, g_b, g_b);
            }

            mesh.vertices.push(Vertex {
                pos: tela.p(vx, vy),
                uv: Pos2::new(u, v),
                color: tela.col(tinta),
            });
        }
    }

    let stride = (COLONNE + 1) as u32;
    for iy in 0..RIGHE as u32 {
        for ix in 0..COLONNE as u32 {
            let i0 = iy * stride + ix;
            let i1 = i0 + 1;
            let i2 = i0 + stride;
            let i3 = i2 + 1;
            mesh.add_triangle(i0, i2, i1);
            mesh.add_triangle(i1, i2, i3);
        }
    }
    painter.add(Shape::mesh(mesh));

    // 4) Palpebra chiusa in stile fumetto durante l'ammiccamento completo
    if chiusura_occhi > 0.55 {
        for (cu, cv) in [(0.415_f32, 0.252_f32), (0.655_f32, 0.258_f32)] {
            let (p_sx_x, p_sx_y) = deforma_uv(cu - 0.058, cv + 0.005);
            let (p_cx_x, p_cx_y) = deforma_uv(cu, cv - 0.004);
            let (p_dx_x, p_dx_y) = deforma_uv(cu + 0.058, cv + 0.006);
            // Copertura palpebrale color pelle della caricatura
            tela.curva3((p_sx_x, p_sx_y - 1.5), (p_cx_x, p_cx_y - 2.0), (p_dx_x, p_dx_y - 1.5), 5.5, PELLE_PALPEBRA);
            tela.curva3((p_sx_x, p_sx_y - 3.2), (p_cx_x, p_cx_y - 4.0), (p_dx_x, p_dx_y - 3.2), 2.5, PELLE_OMBRA);
            // Linea ciglia inchiostrata
            tela.curva3((p_sx_x, p_sx_y), (p_cx_x, p_cx_y + 1.8), (p_dx_x, p_dx_y), 2.4, INCHIOSTRO);
        }
    }

    // 5) Indicatori fumettistici laterali di Anticipo / Ritardo / Panico
    if !stato.applauso {
        disegna_indicatori_ritmo(&tela, stato, &deforma_uv, t);
    }

    // 6) Applauso festoso a fine talk
    if stato.applauso {
        disegna_applauso(&tela, t);
    }

    // 7) Gocce di sudore in stile fumetto quando è nel panico
    if stato.sudore {
        disegna_sudore_comic(&tela, &deforma_uv, t);
    }

    // Fumetto comic per le milestone ("METÀ!", "-5 MIN", "DOMANDE?", "STOP!")
    if let Some(b) = &stato.bandiera {
        if b.alzata > 0.05 {
            let testa_top = deforma_uv(0.56, 0.04);
            disegna_fumetto(&tela, testa_top, b, t);
        }
    }
}

/// Disegna le mani che applaudono in stile fumetto davanti al busto a fine talk.
fn disegna_applauso(tela: &Tela, t: f32) {
    let painter = tela.painter;
    // Battito rapido: 0 = mani unite al centro, 1 = mani aperte
    let onda = (t * 15.0).sin();
    let apertura = onda.abs();
    let impatto = (1.0 - apertura * 2.6).clamp(0.0, 1.0);

    let cx = 100.0;
    let cy = 186.0 - (1.0 - apertura) * 4.0;
    let dist = 4.5 + apertura * 21.0;

    // Raggi e stelline d'impatto in stile comic quando le mani battono insieme
    if impatto > 0.05 {
        for ang_deg in [-65.0_f32, -32.0, 0.0, 32.0, 65.0, 115.0, 148.0, 212.0, 245.0] {
            let rad = ang_deg.to_radians();
            let (s, c) = rad.sin_cos();
            let r0 = 13.0 + (1.0 - impatto) * 4.0;
            let r1 = r0 + 8.5 * impatto;
            let p0 = (cx + c * r0, cy - 8.0 + s * r0 * 0.75);
            let p1 = (cx + c * r1, cy - 8.0 + s * r1 * 0.75);
            tela.linea(p0, p1, 3.2, INCHIOSTRO);
            tela.linea(p0, p1, 1.8, GIALLO);
        }
    }

    // Disegna le due mani (sinistra = -1, destra = +1)
    for lato in [-1.0_f32, 1.0_f32] {
        let hx = cx + lato * dist;
        let hy = cy + apertura * 2.5;
        let inc = -lato * (0.28 + apertura * 0.22);
        let (si, co) = inc.sin_cos();
        let ruota = |dx: f32, dy: f32| -> (f32, f32) {
            (hx + dx * co - dy * si, hy + dx * si + dy * co)
        };

        // Dita (4 dita allungate verso l'alto + pollice laterale)
        for (ox, oy, lungh) in [
            (-5.5 * lato, -7.0, 11.5),
            (-2.0 * lato, -8.5, 13.0),
            (1.8 * lato, -8.0, 12.0),
            (5.2 * lato, -6.5, 10.0),
        ] {
            let b = ruota(ox, oy);
            let p = ruota(ox - lato * 1.5, oy - lungh);
            tela.linea(b, p, 6.8, INCHIOSTRO);
            tela.linea(b, p, 4.4, PELLE_PALPEBRA);
        }

        // Pollice rivolto verso l'esterno/alto
        let pb = ruota(lato * 5.0, -2.0);
        let pp = ruota(lato * 11.5, -10.5);
        tela.linea(pb, pp, 6.8, INCHIOSTRO);
        tela.linea(pb, pp, 4.4, PELLE_PALPEBRA);

        // Palmo della mano con contorno inchiostro
        let p_centro = ruota(0.0, 0.0);
        tela.cerchio(p_centro.0, p_centro.1, 10.8, INCHIOSTRO);
        tela.cerchio(p_centro.0, p_centro.1, 8.6, PELLE_PALPEBRA);
        let p_ombra = ruota(lato * 2.0, 2.2);
        tela.cerchio(p_ombra.0, p_ombra.1, 5.5, PELLE_OMBRA);

        // Lineette di movimento laterali dietro la mano
        let mx = hx + lato * 14.0;
        tela.curva3(
            (mx, hy - 8.0),
            (mx + lato * 3.5, hy),
            (mx, hy + 8.0),
            1.8,
            INCHIOSTRO,
        );
    }

    // Scritte fumettistiche "CLAP!" che rimbalzano ai lati
    let fase_clap = (t * 3.8).sin();
    for (lato_x, sfas) in [(26.0_f32, 1.0_f32), (174.0_f32, -1.0_f32)] {
        let rimbalzo = (fase_clap * sfas).max(0.0);
        let ty = 152.0 - rimbalzo * 8.0;
        let scala_t = 9.5 + rimbalzo * 2.0;
        for (dx, dy) in [(-0.8_f32, 0.8_f32), (0.8, 0.8), (0.0, -0.8)] {
            painter.text(
                tela.p(lato_x + dx, ty + dy),
                Align2::CENTER_CENTER,
                "CLAP!",
                FontId::proportional(tela.s(scala_t)),
                tela.col(INCHIOSTRO),
            );
        }
        painter.text(
            tela.p(lato_x, ty),
            Align2::CENTER_CENTER,
            "CLAP!",
            FontId::proportional(tela.s(scala_t)),
            tela.col(GIALLO),
        );
    }
}

/// Aura luminosa morbida dietro la caricatura per segnalare anticipo (verde) o ritardo (rosso/arancio).
fn disegna_aura(tela: &Tela, stato: &StatoAvatar, t: f32) {
    let pulso = 0.85 + 0.15 * (t * 3.5).sin();
    let (colore, forza) = if stato.alza_rosso > 0.30 {
        (ROSSO, (stato.alza_rosso - 0.25).clamp(0.0, 1.0))
    } else if stato.alza_verde > 0.30 {
        (VERDE, (stato.alza_verde - 0.25).clamp(0.0, 1.0))
    } else {
        return;
    };

    for (r, a_base) in [(76.0, 26.0), (64.0, 42.0), (52.0, 58.0)] {
        let alpha = (a_base * forza * pulso) as u8;
        let c = Color32::from_rgba_unmultiplied(colore.r(), colore.g(), colore.b(), alpha);
        tela.cerchio(100.0, 114.0, r, c);
    }
}

/// Disegna segni grafici in stile comic attorno alla caricatura (frecce verdi in anticipo,
/// linee di tensione / allerta rosse in ritardo).
fn disegna_indicatori_ritmo(
    tela: &Tela,
    stato: &StatoAvatar,
    deforma_uv: &dyn Fn(f32, f32) -> (f32, f32),
    t: f32,
) {
    // Indicatore verde a sinistra quando sei in anticipo
    if stato.alza_verde > 0.35 {
        let forza = ((stato.alza_verde - 0.35) / 0.65).clamp(0.0, 1.0);
        let dy_onda = (t * 3.2).sin() * 3.5;
        let cx = 20.0;
        let cy = 96.0 + dy_onda - forza * 10.0;

        // Bollino fumetto verde con bordo inchiostro e doppia freccia su
        tela.cerchio(cx + 1.2, cy + 1.8, 13.5, INCHIOSTRO);
        tela.cerchio(cx, cy, 13.5, INCHIOSTRO);
        tela.cerchio(cx, cy, 11.4, VERDE);
        // Riflesso sul bollino
        tela.cerchio(cx - 3.5, cy - 3.5, 3.0, Color32::from_white_alpha(110));

        // Doppia freccia "OK / Anticipo"
        for offset_y in [-2.5_f32, 3.5_f32] {
            let y = cy + offset_y;
            tela.linea((cx - 5.0, y + 2.5), (cx, y - 2.5), 2.5, INCHIOSTRO);
            tela.linea((cx, y - 2.5), (cx + 5.0, y + 2.5), 2.5, INCHIOSTRO);
        }
    }

    // Indicatore rosso a destra (e segni di tensione vicino alla tempia) quando sei in ritardo
    if stato.alza_rosso > 0.35 {
        let forza = ((stato.alza_rosso - 0.35) / 0.65).clamp(0.0, 1.0);
        let vibra = if stato.sudore {
            (t * 20.0).sin() * 1.8
        } else {
            (t * 4.5).sin() * 2.5
        };
        let cx = 180.0 + vibra * 0.5;
        let cy = 96.0 - forza * 10.0 + vibra;

        // Bollino fumetto rosso con punto esclamativo
        tela.cerchio(cx + 1.2, cy + 1.8, 13.5, INCHIOSTRO);
        tela.cerchio(cx, cy, 13.5, INCHIOSTRO);
        tela.cerchio(cx, cy, 11.4, ROSSO);
        tela.cerchio(cx - 3.5, cy - 3.5, 3.0, Color32::from_white_alpha(110));

        // Punto esclamativo bianco bordato inchiostro
        tela.linea((cx, cy - 5.5), (cx, cy + 1.2), 4.2, INCHIOSTRO);
        tela.cerchio(cx, cy + 5.2, 2.3, INCHIOSTRO);
        tela.linea((cx, cy - 5.2), (cx, cy + 1.0), 2.4, Color32::WHITE);
        tela.cerchio(cx, cy + 5.2, 1.4, Color32::WHITE);

        // Lineette comic di tensione accanto alla tempia sinistra
        if forza > 0.45 {
            let (tx, ty) = deforma_uv(0.24, 0.16);
            let s = (t * 12.0).sin() * 1.2;
            tela.linea((tx - 4.0, ty - 2.0 + s), (tx - 11.0, ty - 6.0 + s), 2.0, INCHIOSTRO);
            tela.linea((tx - 5.0, ty + 3.0 - s), (tx - 12.0, ty + 2.0 - s), 2.0, INCHIOSTRO);
        }
    }
}

/// Gocce di sudore in stile fumetto (con contorno inchiostro e riflesso) sulla tempia della caricatura.
fn disegna_sudore_comic(
    tela: &Tela,
    deforma_uv: &dyn Fn(f32, f32) -> (f32, f32),
    t: f32,
) {
    let painter = tela.painter;
    // Due gocce sfasate sulla tempia destra e una a sinistra
    for (u_base, v_base, sfasamento, scala_g) in [
        (0.71_f32, 0.13_f32, 0.0_f32, 1.0_f32),
        (0.29_f32, 0.16_f32, 0.48_f32, 0.85_f32),
    ] {
        let fase = (t * 0.95 + sfasamento).fract();
        let (bx, by) = deforma_uv(u_base, v_base + fase * 0.12);
        let r = 4.2 * scala_g;

        // Contorno esterno in inchiostro della goccia a lacrima
        painter.add(Shape::convex_polygon(
            vec![
                tela.p(bx, by - r * 2.4),
                tela.p(bx + r * 1.15, by - r * 0.2),
                tela.p(bx, by + r * 0.8),
                tela.p(bx - r * 1.15, by - r * 0.2),
            ],
            tela.col(INCHIOSTRO),
            Stroke::NONE,
        ));
        tela.cerchio(bx, by, r + 1.5, INCHIOSTRO);

        // Riempimento cel-shaded azzurro
        painter.add(Shape::convex_polygon(
            vec![
                tela.p(bx, by - r * 1.9),
                tela.p(bx + r * 0.88, by - r * 0.1),
                tela.p(bx, by + r * 0.5),
                tela.p(bx - r * 0.88, by - r * 0.1),
            ],
            tela.col(GOCCIA),
            Stroke::NONE,
        ));
        tela.cerchio(bx, by, r, GOCCIA);
        tela.cerchio(bx - r * 0.25, by + r * 0.2, r * 0.65, GOCCIA_SCURA);
        // Riflesso bianco
        tela.cerchio(bx + r * 0.32, by - r * 0.25, r * 0.30, Color32::WHITE);
    }
}

/// Fumetto in stile comic-book per le milestone ("METÀ!", "-5 MIN", "DOMANDE?", "STOP!").
fn disegna_fumetto(tela: &Tela, bersaglio: (f32, f32), b: &Bandiera, t: f32) {
    let alzata = b.alzata.clamp(0.0, 1.0);
    if alzata < 0.08 {
        return;
    }

    let painter = tela.painter;
    let dondola = (t * 5.0).sin() * 1.2;

    // Rettangolo arrotondato del fumetto in alto, sopra la testa
    let larg = 122.0 * alzata;
    let alt = 27.0 * alzata;
    let cx = 100.0;
    let cy = 17.5 + dondola;

    let min_x = cx - larg * 0.5;
    let max_x = cx + larg * 0.5;
    let min_y = cy - alt * 0.5;
    let max_y = cy + alt * 0.5;

    // Ombra netta fumettistica spostata in basso a destra
    let rect_ombra = Rect::from_min_max(
        tela.p(min_x + 2.2, min_y + 2.6),
        tela.p(max_x + 2.2, max_y + 2.6),
    );
    painter.rect_filled(rect_ombra, tela.s(8.0), tela.col(INCHIOSTRO));

    // Codina del fumetto che punta verso la caricatura
    if alzata > 0.5 {
        let px = bersaglio.0.clamp(92.0, 118.0);
        let py = (max_y + 7.5).min(bersaglio.1 - 1.5);
        painter.add(Shape::convex_polygon(
            vec![
                tela.p(cx + 2.0, max_y - 1.0),
                tela.p(cx + 15.0, max_y - 1.0),
                tela.p(px, py),
            ],
            tela.col(b.colore),
            Stroke::new(tela.s(2.4), tela.col(INCHIOSTRO)),
        ));
    }

    // Corpo del fumetto con bordo inchiostro spesso
    let rect_fumetto = Rect::from_min_max(tela.p(min_x, min_y), tela.p(max_x, max_y));
    painter.rect(
        rect_fumetto,
        tela.s(8.0),
        tela.col(b.colore),
        Stroke::new(tela.s(2.4), tela.col(INCHIOSTRO)),
        eframe::egui::StrokeKind::Outside,
    );

    if alzata < 0.45 {
        return;
    }

    // Testo in grassetto fumettistico
    let caratteri = b.testo.chars().count().max(1) as f32;
    let dimensione = (larg * 0.82 / (caratteri * 0.58)).min(16.0);
    let colore_testo = if b.colore == ROSSO {
        Color32::WHITE
    } else {
        INCHIOSTRO
    };

    for dx in [-0.4, 0.4] {
        painter.text(
            tela.p(cx + dx, cy),
            Align2::CENTER_CENTER,
            &b.testo,
            FontId::proportional(tela.s(dimensione)),
            tela.col(colore_testo),
        );
    }
}
