use grafeia_app::app::App;
use wasm_bindgen::prelude::*;
use js_sys::Uint8Array;

#[wasm_bindgen(start)]
pub fn run() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    console_log::init_with_level(log::Level::Debug);
}


#[wasm_bindgen]
pub struct Grafeia(App);

#[wasm_bindgen]
impl Grafeia {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Grafeia {
        Grafeia(App::load().unwrap_or_else(App::build))
    }

    #[wasm_bindgen]
    pub fn show(self) {
        pathfinder_view::show(self.0, pathfinder_view::Config {
            zoom: false,
            pan: true
        });
    }
}
