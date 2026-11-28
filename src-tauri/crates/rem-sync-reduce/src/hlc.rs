use std::cmp::Ordering;
use std::fmt;

use crate::ReduceError;

pub const MAX_HLC_WALL_MS: u64 = 9_999_999_999_999;
pub const MAX_HLC_COUNTER: u16 = 9_999;
pub const DEVICE_ID_LEN: usize = 8;

/// Hybrid Logical Clock formatted as `<epoch_ms_13digits>-<counter_4digits>-<device_id_8chars>`.
///
/// The canonical string representation is strictly lexicographically sortable and matches
/// `(wall_ms, counter, device_id)` ordering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hlc {
    pub wall_ms: u64,
    pub counter: u16,
    pub device_id: String,
}

impl Hlc {
    pub fn new(
        wall_ms: u64,
        counter: u16,
        device_id: impl Into<String>,
    ) -> Result<Self, ReduceError> {
        let device_id = device_id.into();
        if wall_ms > MAX_HLC_WALL_MS {
            return Err(ReduceError::InvalidInput(format!(
                "hlc timestamp out of range: {wall_ms}"
            )));
        }
        if counter > MAX_HLC_COUNTER {
            return Err(ReduceError::InvalidInput(format!(
                "hlc counter out of range: {counter}"
            )));
        }
        validate_device_id(&device_id)?;
        Ok(Self {
            wall_ms,
            counter,
            device_id,
        })
    }

    pub fn parse(raw: &str) -> Result<Self, ReduceError> {
        let bytes = raw.as_bytes();
        if bytes.len() != 27 || bytes[13] != b'-' || bytes[18] != b'-' {
            return Err(ReduceError::InvalidInput(format!(
                "invalid hlc format: {raw}"
            )));
        }
        let wall_part = &raw[0..13];
        let counter_part = &raw[14..18];
        let device_part = &raw[19..27];
        if !wall_part.bytes().all(|b| b.is_ascii_digit())
            || !counter_part.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(ReduceError::InvalidInput(format!(
                "invalid hlc numeric component: {raw}"
            )));
        }
        let wall_ms: u64 = wall_part
            .parse()
            .map_err(|_| ReduceError::InvalidInput(format!("invalid hlc timestamp: {raw}")))?;
        let counter: u16 = counter_part
            .parse()
            .map_err(|_| ReduceError::InvalidInput(format!("invalid hlc counter: {raw}")))?;
        Self::new(wall_ms, counter, device_part)
    }

    /// Produce a strictly greater HLC for a local mutation on `device_id`.
    pub fn tick(last: Option<&Hlc>, now_ms: i64, device_id: &str) -> Result<Self, ReduceError> {
        validate_device_id(device_id)?;
        let physical = now_ms.max(0) as u64;
        if physical > MAX_HLC_WALL_MS {
            return Err(ReduceError::InvalidInput(format!(
                "clock timestamp out of range: {now_ms}"
            )));
        }
        let (wall_ms, counter) = match last {
            Some(prev) if physical > prev.wall_ms => (physical, 0),
            Some(prev) => {
                if prev.counter < MAX_HLC_COUNTER {
                    (prev.wall_ms, prev.counter + 1)
                } else if prev.wall_ms < MAX_HLC_WALL_MS {
                    (prev.wall_ms + 1, 0)
                } else {
                    return Err(ReduceError::InvalidInput(
                        "hlc clock exhausted maximum range".into(),
                    ));
                }
            }
            None => (physical, 0),
        };
        Self::new(wall_ms, counter, device_id)
    }

    pub fn wall_ms_i64(&self) -> i64 {
        self.wall_ms as i64
    }
}

impl Ord for Hlc {
    fn cmp(&self, other: &Self) -> Ordering {
        self.wall_ms
            .cmp(&other.wall_ms)
            .then_with(|| self.counter.cmp(&other.counter))
            .then_with(|| self.device_id.cmp(&other.device_id))
    }
}

impl PartialOrd for Hlc {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Hlc {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:013}-{:04}-{}",
            self.wall_ms, self.counter, self.device_id
        )
    }
}

pub fn validate_device_id(device_id: &str) -> Result<(), ReduceError> {
    if device_id.len() != DEVICE_ID_LEN
        || !device_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return Err(ReduceError::InvalidInput(format!(
            "invalid device id (expected 8 lowercase alphanumeric chars): {device_id}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_parses_canonically() {
        let hlc = Hlc::new(1_732_165_200_000, 7, "a1b2c3d4").unwrap();
        assert_eq!(hlc.to_string(), "1732165200000-0007-a1b2c3d4");
        assert_eq!(Hlc::parse(&hlc.to_string()).unwrap(), hlc);
    }

    #[test]
    fn tick_stays_monotonic_across_same_ms_and_backward_clock_jumps() {
        let first = Hlc::tick(None, 5_000, "aaaaaaaa").unwrap();
        assert_eq!(first.to_string(), "0000000005000-0000-aaaaaaaa");

        let same_ms = Hlc::tick(Some(&first), 5_000, "aaaaaaaa").unwrap();
        assert_eq!(same_ms.to_string(), "0000000005000-0001-aaaaaaaa");
        assert!(same_ms.to_string() > first.to_string());

        let backward = Hlc::tick(Some(&same_ms), 4_000, "00000000").unwrap();
        assert_eq!(backward.to_string(), "0000000005000-0002-00000000");
        assert!(backward.to_string() > same_ms.to_string());

        let ahead = Hlc::tick(Some(&backward), 5_001, "00000000").unwrap();
        assert_eq!(ahead.to_string(), "0000000005001-0000-00000000");
        assert!(ahead.to_string() > backward.to_string());
    }

    #[test]
    fn counter_overflow_rolls_wall_clock_forward_by_one_ms() {
        let max_counter = Hlc::new(100, MAX_HLC_COUNTER, "dev00001").unwrap();
        let rolled = Hlc::tick(Some(&max_counter), 100, "dev00001").unwrap();
        assert_eq!(rolled.wall_ms, 101);
        assert_eq!(rolled.counter, 0);
        assert!(rolled.to_string() > max_counter.to_string());
    }

    #[test]
    fn rejects_malformed_hlc_strings() {
        assert!(Hlc::parse("").is_err());
        assert!(Hlc::parse("123-0-abc").is_err());
        assert!(Hlc::parse("1732165200000-0000-UPPERCAS").is_err());
        assert!(Hlc::parse("173216520000a-0000-a1b2c3d4").is_err());
    }
}
