use std::{
    collections::VecDeque,
    fs::File,
    io::{Read, Seek},
    sync::{Arc, Mutex},
    thread::{self, sleep},
    time::{Duration, Instant, SystemTime},
};

use chrono::{DateTime, Utc};
use color_eyre::eyre::Result as EyreResult;
use iced::{
    time::every,
    widget::{
        canvas::{Cache, Frame, Geometry},
        Column, Container, Row, Scrollable, Text,
    },
    Alignment, Application, Color, Command, Element, Font, Length, Settings, Size, Subscription,
    Theme,
};
use lm_sensors::LMSensors;
use plotters_iced::{Chart, ChartBuilder, ChartWidget, DrawingBackend, Renderer};
use sysinfo::{CpuRefreshKind, RefreshKind, System};

fn main() -> EyreResult<()> {
    Monty::run(Settings::default())?;
    Ok(())
}

struct Monty {
    chart: SystemChart,
}

impl Application for Monty {
    type Executor = tokio::runtime::Runtime;
    type Flags = ();
    type Message = Message;
    type Theme = Theme;

    fn new(_flags: ()) -> (Monty, Command<Self::Message>) {
        (
            Monty {
                chart: SystemChart::default(),
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        String::from("MontY")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        match message {
            Message::Tick => {
                self.chart.update();
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Self::Message> {
        let content = Column::new()
            .spacing(20)
            .align_items(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fill)
            .push(
                Text::new("System Statistics")
                    .size(22)
                    .font(Font::default()),
            )
            .push(self.chart.view());

        Container::new(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(5)
            .center_x()
            .center_y()
            .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        const FPS: u64 = 50;
        every(Duration::from_millis(500 / FPS)).map(|_| Message::Tick)
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }
}

#[derive(Debug)]
enum Message {
    Tick,
}

struct SystemChart {
    sys: System,
    sensors: LMSensors,
    last_sample_time: Instant,
    usage: SimpleChart,
    freq: SimpleChart,
    temp: SimpleChart,
    watts: SimpleChart,
    chart_height: f32,
    current_wattage: Arc<Mutex<i32>>,
}

impl Default for SystemChart {
    fn default() -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::new().with_cpu(CpuRefreshKind::new().with_cpu_usage()),
        );
        let sensors = lm_sensors::Initializer::default().initialize().unwrap();
        let now = Utc::now();
        let cpu_usage = sys.global_cpu_info().cpu_usage();
        let cpu_freq =
            sys.cpus().iter().map(|c| c.frequency()).sum::<u64>() / sys.cpus().len() as u64;
        let pkg_temp = SystemChart::get_package_temp(&sensors);
        let mut msr_file = File::open("/dev/cpu/0/msr").expect("Not enough permissions");

        let current_wattage = Arc::new(Mutex::new(0));

        let inner_wattage = current_wattage.clone();
        thread::spawn(move || {
            let mut msr_res = [0; 8];
            let mut pdraw = 0;
            let mut time = SystemTime::now();
            loop {
                msr_file.seek(std::io::SeekFrom::Start(0x611)).unwrap();
                msr_file.read_exact(&mut msr_res).expect("Bad CPU MSR");
                let new_time = SystemTime::now();
                let new_pdraw = u32::from_le_bytes(msr_res[0..4].try_into().unwrap());
                let time_diff = new_time.duration_since(time).unwrap().as_millis();
                let time_diff = if time_diff == 0 { 1 } else { time_diff };
                let power_diff = (new_pdraw - pdraw) as f64 / 1.53;
                let power_diff = power_diff / 10.0;
                let diff = power_diff as u32 / time_diff as u32;

                *inner_wattage.lock().unwrap() = diff as i32;

                pdraw = new_pdraw;
                time = new_time;
                sleep(Duration::from_millis(100));
            }
        });

        Self {
            sys,
            sensors,
            last_sample_time: Instant::now(),
            usage: SimpleChart::new(vec![(now, cpu_usage as i32)].into_iter(), "%".into(), 100),
            freq: SimpleChart::new(
                vec![(now, cpu_freq as i32)].into_iter(),
                " MHz".into(),
                5000,
            ),
            temp: SimpleChart::new(vec![(now, pkg_temp as i32)].into_iter(), " °C".into(), 100),
            watts: SimpleChart::new(vec![(now, 0)].into_iter(), " W".into(), 80),
            chart_height: 300.0,
            current_wattage,
        }
    }
}

impl SystemChart {
    #[inline]
    fn should_update(&self) -> bool {
        self.last_sample_time.elapsed() > Duration::from_millis(500)
    }

    fn update(&mut self) {
        if !self.should_update() {
            return;
        }

        self.sys.refresh_cpu();
        self.last_sample_time = Instant::now();
        let now = Utc::now();

        let cpu_usage = self.sys.global_cpu_info().cpu_usage();
        let cpu_freq = self.sys.cpus().iter().map(|c| c.frequency()).sum::<u64>()
            / self.sys.cpus().len() as u64;

        let pkg_temp = SystemChart::get_package_temp(&self.sensors);
        let watts = *self.current_wattage.lock().unwrap();

        self.usage.push_data(now, cpu_usage as i32);
        self.freq.push_data(now, cpu_freq as i32);
        self.temp.push_data(now, pkg_temp);
        self.watts.push_data(now, watts);
    }

    fn view(&self) -> Element<Message> {
        let mut col = Column::new()
            .width(Length::Fill)
            .height(Length::Shrink)
            .align_items(Alignment::Center);

        let chart_height = self.chart_height;

        let mut upper_row = Row::new()
            .spacing(15)
            .padding(20)
            .width(Length::Fill)
            .height(Length::Shrink)
            .align_items(Alignment::Center);

        let cpu_freq = self.sys.cpus().iter().map(|c| c.frequency()).sum::<u64>()
            / self.sys.cpus().len() as u64;

        let pkg_temp = SystemChart::get_package_temp(&self.sensors);
        let watts = *self.current_wattage.lock().unwrap();

        upper_row = upper_row.push(self.usage.view(
            format!(
                "CPU 0: {}",
                self.sys.cpus().first().map_or("Generic", |cpu| cpu.brand())
            ),
            chart_height,
            Color::WHITE,
        ));

        let freq_color = if cpu_freq == 399 {
            Color::from_rgb8(240, 0, 0)
        } else {
            Color::WHITE
        };

        upper_row = upper_row.push(self.freq.view(
            format!("Frequency: {} MHz", cpu_freq),
            chart_height,
            freq_color,
        ));

        col = col.push(upper_row);

        let mut lower_row = Row::new()
            .spacing(15)
            .padding(20)
            .width(Length::Fill)
            .height(Length::Shrink)
            .align_items(Alignment::Center);

        lower_row = lower_row.push(self.temp.view(
            format!("Temperature: {} °C", pkg_temp),
            chart_height,
            Color::WHITE,
        ));

        lower_row = lower_row.push(self.watts.view(
            format!("Power Draw: {} W", watts),
            chart_height,
            Color::WHITE,
        ));

        col = col.push(lower_row);

        Scrollable::new(col).height(Length::Shrink).into()
    }

    fn get_package_temp(sensors: &LMSensors) -> i32 {
        sensors
            .chip_iter(None)
            .find(|ch| ch.name().is_ok_and(|n| n.contains("coretemp-isa-0000")))
            .and_then(|ch| {
                ch.feature_iter().find(|f| {
                    f.name()
                        .is_some_and(|n| n.is_ok_and(|n| n.contains("temp1")))
                })
            })
            .and_then(|ft| {
                ft.sub_feature_by_kind(lm_sensors::value::Kind::TemperatureInput)
                    .ok()
            })
            .and_then(|sf| sf.value().ok())
            .map(|v| v.raw_value() as i32)
            .unwrap_or_default()
    }
}

struct SimpleChart {
    cache: Cache,
    data_points: VecDeque<(DateTime<Utc>, i32)>,
    limit: Duration,
    unit: String,
    max_value: i32,
}

impl SimpleChart {
    fn new(data: impl Iterator<Item = (DateTime<Utc>, i32)>, unit: String, max_value: i32) -> Self {
        let data_points: VecDeque<_> = data.collect();
        Self {
            cache: Cache::new(),
            data_points,
            limit: Duration::from_secs(60),
            unit,
            max_value,
        }
    }

    fn push_data(&mut self, time: DateTime<Utc>, value: i32) {
        let cur_ms = time.timestamp_millis();
        self.data_points.push_front((time, value));
        loop {
            if let Some((time, _)) = self.data_points.back() {
                let diff = Duration::from_millis((cur_ms - time.timestamp_millis()) as u64);
                if diff > self.limit {
                    self.data_points.pop_back();
                    continue;
                }
            }
            break;
        }
        self.cache.clear();
    }

    fn view(&self, title: String, chart_height: f32, color: Color) -> Element<Message> {
        Column::new()
            .width(Length::Fill)
            .height(Length::Shrink)
            .spacing(5)
            .align_items(Alignment::Center)
            .push(Text::new(title).style(color))
            .push(ChartWidget::new(self).height(Length::Fixed(chart_height)))
            .into()
    }
}

impl Chart<Message> for SimpleChart {
    type State = ();

    #[inline]
    fn draw<R: Renderer, F: Fn(&mut Frame)>(
        &self,
        renderer: &R,
        bounds: Size,
        draw_fn: F,
    ) -> Geometry {
        renderer.draw_cache(&self.cache, bounds, draw_fn)
    }

    fn build_chart<DB: DrawingBackend>(&self, _state: &Self::State, mut chart: ChartBuilder<DB>) {
        use plotters::prelude::*;

        const PLOT_LINE_COLOR: RGBColor = RGBColor(0, 175, 255);

        // Acquire time range
        let newest_time = self
            .data_points
            .front()
            .unwrap_or(&(DateTime::default(), 0))
            .0;

        let oldest_time = newest_time - chrono::Duration::seconds(60);
        let mut chart = chart
            .x_label_area_size(0)
            .y_label_area_size(16 * self.max_value.to_string().len() as i32)
            .margin(20)
            .build_cartesian_2d(oldest_time..newest_time, 0..self.max_value)
            .expect("failed to build chart");

        chart
            .configure_mesh()
            .bold_line_style(plotters::style::colors::WHITE.mix(0.1))
            .light_line_style(plotters::style::colors::WHITE.mix(0.02))
            .axis_style(ShapeStyle::from(plotters::style::colors::WHITE.mix(0.45)).stroke_width(1))
            .y_labels(10)
            .y_label_style(
                ("sans-serif", 15)
                    .into_font()
                    .color(&plotters::style::colors::WHITE.mix(0.65))
                    .transform(FontTransform::Rotate90),
            )
            .y_label_formatter(&|y| format!("{}{}", y, self.unit))
            .draw()
            .expect("failed to draw chart mesh");

        chart
            .draw_series(
                AreaSeries::new(
                    self.data_points.iter().map(|x| (x.0, x.1)),
                    0,
                    PLOT_LINE_COLOR.mix(0.175),
                )
                .border_style(ShapeStyle::from(PLOT_LINE_COLOR).stroke_width(2)),
            )
            .expect("failed to draw chart data");
    }
}
