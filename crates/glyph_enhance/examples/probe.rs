use glyph_enhance::enhance::{STEP_OFFSET, Technique, faithfulness, prepare_with};
use glyph_enhance::layers::Layers;
use glyph_enhance::trace::TraceParameters;
fn main() {
    let font = scene::font::Font::load(std::path::Path::new("../CrystalOSD/Resources/FNTOSD")).unwrap();
    let params = TraceParameters::default();
    let prepared: Vec<_> = (0x21..0x7f).map(|c| prepare_with(&Layers::of_glyph(&font, &font.glyph(c).unwrap()), &params, true)).collect();
    println!("step offset {STEP_OFFSET}");
    for tech in Technique::ALL {
        let (mut ge, mut ae) = (0.0, 0.0);
        for p in &prepared {
            let f = faithfulness(p, tech, 4);
            ge += f.grey_edge_mean;
            ae += f.alpha_edge_mean;
        }
        let n = prepared.len() as f64;
        println!("{:36} grey edge {:.4} alpha edge {:.4}", tech.name(), ge / n, ae / n);
    }
}
