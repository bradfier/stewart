use std::time::Duration;

#[derive(Debug, Clone)]
pub enum Strategy {
    SingleStint(StrategyInner),
    LongStints(StrategyInner),
    EqualStints(StrategyInner),
}

impl Strategy {
    pub fn as_discord_text(&self) -> String {
        match self {
            Strategy::SingleStint(inner) => inner.as_discord_text(),
            Strategy::LongStints(inner) => inner.as_discord_text(),
            Strategy::EqualStints(inner) => inner.as_discord_text(),
        }
    }

    pub fn discord_title(&self) -> &str {
        match self {
            Strategy::SingleStint(_) => "Single Stint",
            Strategy::LongStints(_) => "Longer Stints",
            Strategy::EqualStints(_) => "Equal Stints",
        }
    }
}

impl StrategyInner {
    fn as_discord_text(&self) -> String {
        let mut output = String::new();
        output.push_str("**Starting Fuel**\n");
        output.push_str(&format!(
            "{} L\n{} Laps",
            self.stints[0].fuel_required, self.stints[0].laps
        ));
        for (i, stint) in self.stints.iter().enumerate() {
            output.push_str(&format!(
                "\n\n**Stint {}**\n{}",
                i + 1,
                humantime::format_duration(stint.duration)
            ));
            if i < self.stops.len() {
                output.push_str(&format!(
                    "\n\n**Stop {}**\nLap {}\nAdd fuel: {} L",
                    i + 1,
                    self.stops[i].lap,
                    self.stops[i].fuel_to_add
                ));
            }
        }
        output
    }
}

#[derive(Debug, Clone)]
pub struct StrategyInner {
    pub stints: Vec<Stint>,
    pub stops: Vec<Stop>,
}

#[derive(Debug, Clone)]
pub struct StrategyInput {
    pub race_duration: Duration,
    pub avg_laptime: Duration,
    pub fuel_per_lap: f64,
    pub fuel_capacity: u32,
    pub mandatory_pits: Option<u8>,
    pub permitted_max_stint_length: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct Stint {
    pub duration: Duration,
    pub laps: u32,
    pub fuel_required: u32,
}

#[derive(Debug, Clone)]
pub struct Stop {
    pub lap: u32,
    pub fuel_to_add: u32,
}

impl StrategyInput {
    fn fuel_duration(&self, fuel: u32) -> Duration {
        // Intentionally truncate the laps value here by discarding the fractional part
        let laps = (fuel as f64 / self.fuel_per_lap) as u32;
        laps * self.avg_laptime
    }

    fn max_fuel_duration(&self) -> Duration {
        self.fuel_duration(self.fuel_capacity)
    }

    fn fuel_for_stint(&self, length: Duration) -> u32 {
        let laps = (length.as_secs_f64() / self.avg_laptime.as_secs_f64()).ceil();
        (laps * self.fuel_per_lap).ceil() as u32
    }

    /// The longest possible stint duration based on regulations and fuel capacity
    fn max_stint_time(&self) -> Duration {
        if let Some(stint_time) = self.permitted_max_stint_length {
            std::cmp::min(stint_time, self.max_fuel_duration())
        } else {
            self.max_fuel_duration()
        }
    }

    /// How many stints are required based only on fuel consumption and capacity
    fn fuel_required_stints(&self) -> u8 {
        let stints = self.race_duration.as_secs_f64() / self.max_fuel_duration().as_secs_f64();
        stints.ceil() as u8
    }

    /// How many stints are required given the number of mandatory pits in the input
    fn mandatory_pits_required_stints(&self) -> u8 {
        if let Some(required) = self.mandatory_pits {
            required + 1
        } else {
            1
        }
    }

    /// How many stints are required given the maximum permitted stint length in the input
    fn permitted_stint_length_required_stints(&self) -> u8 {
        if let Some(max) = self.permitted_max_stint_length {
            (self.race_duration.as_secs_f64() / max.as_secs_f64()).ceil() as u8
        } else {
            1
        }
    }

    /// How many stints are required, taking into account fuel and regulations
    fn required_stints(&self) -> u8 {
        std::cmp::max(
            std::cmp::max(
                self.fuel_required_stints(),
                self.mandatory_pits_required_stints(),
            ),
            self.permitted_stint_length_required_stints(),
        )
    }

    // If we require more stints than the car's endurance allows, these must be mandatory
    // stops which are assumed to involve a tyre change. In these circumstances, running long
    // stints is of no advantage so we should show only the even-stints model
    fn all_pits_mandatory(&self) -> bool {
        let time_required_stints =
            (self.race_duration.as_secs_f64() / self.max_stint_time().as_secs_f64()).ceil() as u32;

        if let Some(mandatory) = self.mandatory_pits {
            (time_required_stints - 1) <= mandatory as u32
        } else {
            false
        }
    }

    fn calculate_stints(&self, target_stint_time: Duration) -> Vec<Stint> {
        let mut remaining_race_time = self.race_duration;

        let mut stints = vec![];
        while remaining_race_time.as_secs() != 0 {
            let this_stint_time = std::cmp::min(remaining_race_time, target_stint_time);
            stints.push(Stint {
                duration: this_stint_time,
                fuel_required: self.fuel_for_stint(this_stint_time),
                laps: (this_stint_time.as_secs_f64() / self.avg_laptime.as_secs_f64()).ceil()
                    as u32,
            });

            // This can panic if there's an overflowing subtraction, so just zero it if that would occur
            remaining_race_time = if this_stint_time >= remaining_race_time {
                Duration::new(0, 0)
            } else {
                remaining_race_time - this_stint_time
            };
        }

        stints
    }

    fn calculate_even_stint_strategy(&self) -> Strategy {
        let remaining_required_stints = self.required_stints();
        let target_stint_time = Duration::from_secs_f64(
            (self.race_duration.as_secs_f64() / remaining_required_stints as f64).ceil(),
        );

        let stints = self.calculate_stints(target_stint_time);
        let mut stops: Vec<Stop> = vec![];
        for i in 1..stints.len() {
            let stint_laps = (stints[i - 1].duration.as_secs_f64() / self.avg_laptime.as_secs_f64())
                .ceil() as u32;
            let previous_laps = if let Some(last_stop) = stops.last() {
                last_stop.lap
            } else {
                0
            };
            stops.push(Stop {
                lap: previous_laps + stint_laps,
                fuel_to_add: stints[i].fuel_required,
            });
        }

        Strategy::EqualStints(StrategyInner { stints, stops })
    }

    fn calculate_long_stint_strategy(&self) -> Strategy {
        let target_stint_time = self.max_stint_time();
        let stints = self.calculate_stints(target_stint_time);

        let mut stops: Vec<Stop> = vec![];
        for i in 1..stints.len() {
            let stint_laps = (stints[i - 1].duration.as_secs_f64() / self.avg_laptime.as_secs_f64())
                .ceil() as u32;
            let previous_laps = if let Some(last_stop) = stops.last() {
                last_stop.lap
            } else {
                0
            };
            stops.push(Stop {
                lap: previous_laps + stint_laps,
                fuel_to_add: stints[i].fuel_required,
            });
        }

        Strategy::LongStints(StrategyInner { stints, stops })
    }

    fn calculate_single_stint(&self) -> Strategy {
        let target_stint_time = self.max_stint_time();
        let stints = self.calculate_stints(target_stint_time);

        Strategy::SingleStint(StrategyInner {
            stints,
            stops: vec![],
        })
    }

    pub fn calculate(&self) -> Vec<Strategy> {
        // If a single stint is possible, return that alone
        if self.required_stints() == 1 {
            return vec![self.calculate_single_stint()];
        }

        let mut result = vec![];
        if !self.all_pits_mandatory() {
            // Running long stints is worthwhile as the last pitstops don't require a tyre change
            result.push(self.calculate_long_stint_strategy());
        }
        result.push(self.calculate_even_stint_strategy());
        result
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_fuel_req_only() {
        let mut input = StrategyInput {
            race_duration: Duration::new(7200, 0), // 2 hrs
            avg_laptime: Duration::new(138, 0),    // 2:18
            fuel_per_lap: 3.93,
            fuel_capacity: 110,
            permitted_max_stint_length: None,
            mandatory_pits: None,
        };

        assert_eq!(2, input.required_stints());

        input.race_duration = Duration::new(600, 0); // 10m
        assert_eq!(1, input.required_stints());
    }

    #[test]
    fn test_stints_mandatory_pit() {
        let mut input = StrategyInput {
            race_duration: Duration::new(7200, 0), // 2 hrs
            avg_laptime: Duration::new(138, 0),    // 2:18
            fuel_per_lap: 3.93,
            fuel_capacity: 110,
            permitted_max_stint_length: None,
            mandatory_pits: Some(2),
        };

        assert_eq!(3, input.required_stints());

        input.race_duration = Duration::new(600, 0); // 10m
        assert_eq!(3, input.required_stints());
    }

    #[test]
    fn test_stints_max_time() {
        let input = StrategyInput {
            race_duration: Duration::new(7200, 0), // 2 hrs
            avg_laptime: Duration::new(138, 0),    // 2:18
            fuel_per_lap: 3.93,
            fuel_capacity: 125,
            permitted_max_stint_length: Some(Duration::new(3540, 0)), // 55mins
            mandatory_pits: None,
        };

        assert_eq!(3, input.required_stints());
    }

    #[test]
    fn test_all_mandatory_pits() {
        let mut input = StrategyInput {
            race_duration: Duration::new(7200, 0), // 2 hrs
            avg_laptime: Duration::new(138, 0),    // 2:18
            fuel_per_lap: 3.93,
            fuel_capacity: 125,
            permitted_max_stint_length: Some(Duration::new(3540, 0)), // 55mins
            mandatory_pits: Some(1),
        };
        assert_eq!(false, input.all_pits_mandatory());

        input.mandatory_pits = Some(2);
        assert_eq!(true, input.all_pits_mandatory());
    }

    #[test]
    fn calculate_even_stints_strategy() {
        let input = StrategyInput {
            race_duration: Duration::new(8640, 0), // 2 hrs
            avg_laptime: Duration::new(138, 0),    // 2:18
            fuel_per_lap: 3.93,
            fuel_capacity: 125,
            permitted_max_stint_length: Some(Duration::new(3540, 0)), // 55mins
            mandatory_pits: None,
        };

        let result = input.calculate_even_stint_strategy();
        dbg!(&result);
        match result {
            Strategy::EqualStints(strat) => {
                assert_eq!(3, strat.stints.len());
                assert_eq!(2, strat.stops.len());

                assert_eq!(21, strat.stops[0].lap);
            }
            _ => panic!("Got wrong enum variant"),
        }
    }

    #[test]
    fn calculate_long_stint_strategy() {
        let input = StrategyInput {
            race_duration: Duration::new(14400, 0), // 4 hrs
            avg_laptime: Duration::new(138, 0),     // 2:18
            fuel_per_lap: 3.25,
            fuel_capacity: 110,
            permitted_max_stint_length: None,
            mandatory_pits: Some(3),
        };

        let result = input.calculate_long_stint_strategy();
        match result {
            Strategy::LongStints(strat) => {
                assert_eq!(4, strat.stints.len());
                assert_eq!(3, strat.stops.len());

                assert_eq!(33, strat.stops[0].lap);
                assert_eq!(108, strat.stops[0].fuel_to_add);
            }
            _ => panic!("Got wrong enum variant"),
        }
    }

    #[test]
    fn calculate_simple_strategy() {
        let input = StrategyInput {
            race_duration: Duration::new(3600, 0), // 1 hrs
            avg_laptime: Duration::new(138, 0),    // 2:18
            fuel_per_lap: 3.25,
            fuel_capacity: 110,
            permitted_max_stint_length: None,
            mandatory_pits: None,
        };

        let result = input.calculate();
        assert_eq!(1, result.len());
        match &result[0] {
            Strategy::SingleStint(strat) => {
                assert_eq!(strat.stints[0].fuel_required, 88);
            }
            _ => panic!("Got wrong type of strategy for single stint race"),
        }
    }

    #[test]
    fn calculates_both_strategies() {
        let input = StrategyInput {
            race_duration: Duration::new(14400, 0), // 4 hrs
            avg_laptime: Duration::new(138, 0),     // 2:18
            fuel_per_lap: 3.90,
            fuel_capacity: 110,
            permitted_max_stint_length: None,
            mandatory_pits: None,
        };

        let result = input.calculate();
        assert_eq!(2, result.len());
    }

    #[test]
    fn calculates_only_one_strategy_where_appropriate() {
        let input = StrategyInput {
            race_duration: Duration::new(14400, 0), // 4 hrs
            avg_laptime: Duration::new(138, 0),     // 2:18
            fuel_per_lap: 3.90,
            fuel_capacity: 110,
            permitted_max_stint_length: None,
            mandatory_pits: Some(3), // All pitstops are tyre changes, so the long-stints strategy doesn't make sense
        };

        let result = input.calculate();
        assert_eq!(1, result.len());
    }
}
