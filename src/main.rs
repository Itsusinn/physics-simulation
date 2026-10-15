#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(any(target_arch = "wasm32", test))]
mod gpu;
#[cfg(any(target_arch = "wasm32", test))]
mod model;

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast as _;
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas = document
            .get_element_by_id("simulation")
            .unwrap()
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .unwrap();
        let mut options = eframe::WebOptions::default();
        options.wgpu_options.wgpu_setup =
            eframe::egui_wgpu::WgpuSetup::CreateNew(eframe::egui_wgpu::WgpuSetupCreateNew {
                instance_descriptor: eframe::wgpu::InstanceDescriptor {
                    backends: eframe::wgpu::Backends::BROWSER_WEBGPU,
                    ..eframe::wgpu::InstanceDescriptor::new_without_display_handle()
                },
                ..eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle()
            });
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(|cc| Ok(Box::new(app::ThreeBodyApp::new(cc)?))),
            )
            .await;
        let loading = document.get_element_by_id("loading").unwrap();
        match result {
            Ok(()) => { let _ = loading.set_attribute("style", "display:none"); }
            Err(error) => loading.set_text_content(Some(&format!(
                "WebGPU could not start: {error:?}. Open this page on localhost or HTTPS in a browser with WebGPU enabled."
            ))),
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("This application runs in your browser. Start it with: trunk serve --release");
}
