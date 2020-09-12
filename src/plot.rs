use circular_queue::CircularQueue;
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use plotters::prelude::*;
use plotters_bitmap::bitmap_pixel::BGRXPixel;
use plotters_bitmap::BitMapBackend;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::error::Error;
use std::time::SystemTime;

const W: usize = 480;
const H: usize = 320;

//const SAMPLE_RATE: f64 = 10_000.0;
const FRAME_RATE: f64 = 30.0;
//const WINDOW_TIME: f32 = 5.0;

pub struct Plotter {
    buf: Vec<u8>,
    pub window: Window,
    cs: plotters::chart::ChartState<
        Cartesian2d<plotters::coord::types::RangedCoordf32, plotters::coord::types::RangedCoordf32>,
    >,
    last_flushed: std::time::Instant,
    pub data: CircularQueue<(f32, f32, f32, f32)>,
    window_time: f32,
}

impl Plotter {
    pub fn new(
        window_time: f32,
        minz: f32,
        maxz: f32,
        data_size: usize,
    ) -> Result<Plotter, anyhow::Error> {
        let mut buf = vec![0u8; W * H * 4];
        let root =
            BitMapBackend::<BGRXPixel>::with_buffer_and_format(&mut buf[..], (W as u32, H as u32))?
                .into_drawing_area();
        root.fill(&BLACK)?;
        let mut chart = ChartBuilder::on(&root)
            .margin(10)
            .set_all_label_area_size(30)
            .build_cartesian_2d(0.0_f32..window_time, minz..maxz)?;

        chart
            .configure_mesh()
            .label_style(("sans-serif", 15).into_font().color(&GREEN))
            .axis_style(&GREEN)
            .draw()?;

        let cs = chart.into_chart_state();
        drop(root);
        Ok(Plotter {
            window: Window::new("Test window", W, H, WindowOptions::default())?,
            buf: buf,
            cs: cs,
            last_flushed: std::time::Instant::now(),
            data: CircularQueue::with_capacity(data_size),
            window_time: window_time,
        })
    }

    pub fn tick(&mut self) -> Result<(), anyhow::Error> {
        if self.last_flushed.elapsed().as_millis() > ((1000.0 / FRAME_RATE) as u128) {
            let root = BitMapBackend::<BGRXPixel>::with_buffer_and_format(
                &mut self.buf[..],
                (W as u32, H as u32),
            )?
            .into_drawing_area();
            let mut chart = self.cs.clone().restore(&root);
            chart.plotting_area().fill(&BLACK)?;

            chart
                .configure_mesh()
                .bold_line_style(&GREEN.mix(0.2))
                .light_line_style(&TRANSPARENT)
                .draw()?;

            let latest_time = self.data.iter().next().map(|x| x.0).unwrap_or_default();
            let window_time = self.window_time;
            chart.draw_series(self.data.iter().zip(self.data.iter().skip(1)).map(
                |(&(x0, y0, _, _), &(x1, y1, _, _))| {
                    PathElement::new(
                        vec![(x0 % window_time, y0), (x0 % window_time + (x1 - x0), y1)],
                        &RED.mix(((x0 - latest_time) * 2.0).exp().into()),
                    )
                },
            ))?;
            chart.draw_series(self.data.iter().zip(self.data.iter().skip(1)).map(
                |(&(x0, _, y0, _), &(x1, _, y1, _))| {
                    PathElement::new(
                        vec![(x0 % window_time, y0), (x0 % window_time + (x1 - x0), y1)],
                        &GREEN.mix(((x0 - latest_time) * 2.0).exp().into()),
                    )
                },
            ))?;
            chart.draw_series(self.data.iter().zip(self.data.iter().skip(1)).map(
                |(&(x0, _, _, y0), &(x1, _, _, y1))| {
                    PathElement::new(
                        vec![(x0 % window_time, y0), (x0 % window_time + (x1 - x0), y1)],
                        &BLUE.mix(((x0 - latest_time) * 2.0).exp().into()),
                    )
                },
            ))?;

            drop(root);
            drop(chart);

            if let Some(keys) = self.window.get_keys_pressed(KeyRepeat::Yes) {
                for key in keys {
                    match key {
                        Key::Equal => {
                            //fy += 0.1;
                        }
                        Key::Minus => {
                            //fy -= 0.1;
                        }
                        Key::Key0 => {
                            //fx += 0.1;
                        }
                        Key::Key9 => {
                            //fx -= 0.1;
                        }
                        _ => {
                            continue;
                        }
                    }
                    break;
                }
            }
            let mut buf2 = unsafe { std::slice::from_raw_parts(&self.buf[0] as *const _ as *const _, H * W) };
            self.window.update_with_buffer(&buf2)?;
            self.last_flushed = std::time::Instant::now();
        };

        Ok(())
    }
}
