//! Full dates use only the existing scenario condition grammar.
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Date {
    pub year: i64,
    pub month: i64,
    pub stage: i64,
}

impl Date {
    pub fn new(year: i64, month: i64, stage: i64) -> Option<Self> {
        ((1..=9).contains(&year) && (1..=12).contains(&month) && (1..=3).contains(&stage))
            .then_some(Self { year, month, stage })
    }

    fn bound(self, from: bool) -> Value {
        let year = json!({"source":"time","key":"year","op":"eq","value":self.year});
        let month = json!({"source":"time","key":"month","op":"eq","value":self.month});
        json!({"any":[
            {"source":"time","key":"year","op":if from {"gt"} else {"lt"},"value":self.year},
            {"all":[year,{"source":"time","key":"month","op":if from {"gt"} else {"lt"},"value":self.month}]},
            {"all":[year,month,{"source":"time","key":"stage","op":if from {"ge"} else {"le"},"value":self.stage}]}
        ]})
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DateRange {
    pub from: Option<Date>,
    pub until: Option<Date>,
}

impl DateRange {
    pub fn write(self) -> Value {
        let mut all = vec![];
        if let Some(from) = self.from {
            all.push(from.bound(true));
        }
        if let Some(until) = self.until {
            all.push(until.bound(false));
        }
        json!({"all":all})
    }

    pub fn is_reversed(self) -> bool {
        self.from
            .zip(self.until)
            .is_some_and(|(from, until)| from > until)
    }
}

/// Recognize our exact tree, preserving arbitrary advanced conditions verbatim.
pub(super) fn read_bound(value: &Value) -> Option<(Date, bool)> {
    let children = value.get("any")?.as_array()?;
    if children.len() != 3 {
        return None;
    }
    let date = Date::new(
        children[0]["value"].as_i64()?,
        children[1]["all"][1]["value"].as_i64()?,
        children[2]["all"][2]["value"].as_i64()?,
    )?;
    for from in [true, false] {
        if value == &date.bound(from) {
            return Some((date, from));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test(value: &Value, current: Date) -> bool {
        if let Some(all) = value["all"].as_array() {
            return all.iter().all(|v| test(v, current));
        }
        if let Some(any) = value["any"].as_array() {
            return any.iter().any(|v| test(v, current));
        }
        let actual = match value["key"].as_str().unwrap() {
            "year" => current.year,
            "month" => current.month,
            "stage" => current.stage,
            _ => panic!(),
        };
        let wanted = value["value"].as_i64().unwrap();
        match value["op"].as_str().unwrap() {
            "eq" => actual == wanted,
            "gt" => actual > wanted,
            "ge" => actual >= wanted,
            "lt" => actual < wanted,
            "le" => actual <= wanted,
            _ => panic!(),
        }
    }

    fn dates() -> impl Iterator<Item = Date> {
        (1..=9).flat_map(|year| {
            (1..=12).flat_map(move |month| (1..=3).map(move |stage| Date { year, month, stage }))
        })
    }

    #[test]
    fn inclusive_boundaries_match_every_game_date_across_years() {
        for boundary in dates() {
            let lower = DateRange {
                from: Some(boundary),
                until: None,
            }
            .write();
            let upper = DateRange {
                from: None,
                until: Some(boundary),
            }
            .write();
            for current in dates() {
                assert_eq!(
                    test(&lower, current),
                    current >= boundary,
                    "from {boundary:?}, current {current:?}"
                );
                assert_eq!(
                    test(&upper, current),
                    current <= boundary,
                    "until {boundary:?}, current {current:?}"
                );
            }
        }
    }

    #[test]
    fn full_range_includes_endpoints_and_rejects_reversed_dates() {
        let from = Date::new(2, 12, 3).unwrap();
        let until = Date::new(3, 2, 1).unwrap();
        let range = DateRange {
            from: Some(from),
            until: Some(until),
        };
        for current in dates() {
            assert_eq!(
                test(&range.write(), current),
                current >= from && current <= until
            );
        }
        assert!(!range.is_reversed());
        assert!(DateRange {
            from: Some(until),
            until: Some(from)
        }
        .is_reversed());
        for date in [
            (0, 1, 1),
            (10, 1, 1),
            (1, 0, 1),
            (1, 13, 1),
            (1, 1, 0),
            (1, 1, 4),
        ] {
            assert!(Date::new(date.0, date.1, date.2).is_none());
        }
    }

    #[test]
    fn only_exact_supported_trees_are_recognized() {
        let date = Date::new(2, 3, 1).unwrap();
        for from in [true, false] {
            let mut bound = date.bound(from);
            assert_eq!(read_bound(&bound), Some((date, from)));
            bound["any"][2]["all"][2]["value"] = json!(4);
            assert!(read_bound(&bound).is_none());
        }
        let mut different = date.bound(true);
        different["any"][1]["all"][1]["op"] = json!("ge");
        assert!(read_bound(&different).is_none());
        assert!(read_bound(&json!({"any":[]})).is_none());
    }
}
