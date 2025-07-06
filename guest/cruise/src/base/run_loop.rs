use crucible::base::env::current_time;

/// A fixed time-step abstraction à la ["Fix your time-step"](fyt).
///
/// [fyt]: https://gafferongames.com/post/fix_your_timestep/
#[derive(Debug)]
pub struct StepTimer {
    dt: f64,
    last_tick: f64,
    accumulator: f64,
}

impl StepTimer {
    pub fn new(dt: f64) -> Self {
        Self {
            dt,
            last_tick: current_time(),
            accumulator: 0.0,
        }
    }

    pub fn dt(&self) -> f64 {
        self.dt
    }

    pub fn set_dt(&mut self, dt: f64) {
        self.dt = dt;
    }

    pub fn tick(&mut self) -> StepResult {
        let new_time = current_time();
        let frame_time = new_time - self.last_tick;
        self.last_tick = new_time;

        self.accumulator += frame_time;
        self.accumulator = self.accumulator.min(0.25);

        let times_ticked = self.accumulator / self.dt;
        self.accumulator %= self.dt;

        StepResult {
            times_ticked: times_ticked as u32,
            wait_until: current_time() + (self.dt - self.accumulator),
        }
    }

    pub fn render_alpha(&self) -> f64 {
        self.accumulator
    }
}

#[derive(Debug, Clone)]
pub struct StepResult {
    pub times_ticked: u32,
    pub wait_until: f64,
}
