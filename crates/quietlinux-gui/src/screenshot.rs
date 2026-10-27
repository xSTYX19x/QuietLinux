//! `--screenshot FILE`: the window of the demo mode, drawn by the software renderer
//! into a PPM image without a screen. Used for the docs and to check the layout.

use std::error::Error;
use std::path::Path;
use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize, Rgb8Pixel};

use crate::backend::{Backend, Request, State};
use crate::{AppWindow, CONTROLLER, Controller, set_up, with_controller};

const WIDTH: u32 = 1100;
const HEIGHT: u32 = 720;

struct Offscreen(Rc<MinimalSoftwareWindow>);

impl Platform for Offscreen {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

pub fn run(path: &Path, page: i32) -> Result<(), Box<dyn Error>> {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Offscreen(window.clone())))
        .map_err(|e| format!("{e:?}"))?;
    let ui = AppWindow::new()?;
    set_up(&ui);
    window.set_size(PhysicalSize::new(WIDTH, HEIGHT));
    CONTROLLER.with(|c| {
        *c.borrow_mut() = Some(Controller {
            ui: ui.as_weak(),
            requests: None,
            state: State::default(),
            editor_layer: 0,
        });
    });
    let outcome = Backend::demo().handle(Request::Refresh);
    with_controller(|c, ui| c.apply(ui, true, outcome));
    ui.set_page(page);
    match page {
        1 => with_controller(|c, ui| c.select_key(ui, "M5")),
        2 => {
            ui.set_macro_current(0);
            with_controller(|c, ui| c.select_macro(ui, 0));
        }
        _ => {}
    }
    ui.show()?;
    slint::platform::update_timers_and_animations();
    let (w, h) = (WIDTH as usize, HEIGHT as usize);
    let mut pixels = vec![Rgb8Pixel::new(0, 0, 0); w * h];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut pixels, w);
    });
    let mut image = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    for p in &pixels {
        image.extend_from_slice(&[p.r, p.g, p.b]);
    }
    std::fs::write(path, image)?;
    Ok(())
}
