/// Integrated loudness in LUFS and true peak in dBTP; None where the audio is silent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Level {
    pub loudness: Option<f64>,
    pub peak: Option<f64>,
}

impl Level {
    /// Several tracks as one: the loudest loudness and the highest peak.
    pub fn loudest(levels: &[Level]) -> Level {
        let max = |it: &mut dyn Iterator<Item = f64>| it.reduce(f64::max);
        Level {
            loudness: max(&mut levels.iter().filter_map(|l| l.loudness)),
            peak: max(&mut levels.iter().filter_map(|l| l.peak)),
        }
    }
}

fn value_after(line: &str, label: &str) -> Option<f64> {
    let rest = line.trim().strip_prefix(label)?;
    rest.split_whitespace().next()?.parse::<f64>().ok()
}

pub fn parse_ebur128(log: &str) -> Option<Level> {
    let summary = &log[log.rfind("Summary:")?..];
    let mut level = Level::default();
    let mut seen = false;
    for line in summary.lines() {
        if let Some(i) = value_after(line, "I:") {
            seen = true;
            level.loudness = (i > -70.0).then_some(i);
        } else if let Some(p) = value_after(line, "Peak:") {
            level.peak = p.is_finite().then_some(p);
        }
    }
    seen.then_some(level)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loudnorm {
    pub input_i: f64,
    pub input_tp: f64,
    pub input_lra: f64,
    pub input_thresh: f64,
    pub target_offset: f64,
}

/// None for silence too: the values are not finite.
pub fn parse_loudnorm(log: &str) -> Option<Loudnorm> {
    let start = log.rfind("\n{").map(|i| i + 1).or_else(|| log.find('{'))?;
    let end = start + log[start..].find('}')? + 1;
    let v: serde_json::Value = serde_json::from_str(&log[start..end]).ok()?;
    let num = |k: &str| {
        v.get(k)?
            .as_str()?
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|x| x.is_finite())
    };
    Some(Loudnorm {
        input_i: num("input_i")?,
        input_tp: num("input_tp")?,
        input_lra: num("input_lra")?,
        input_thresh: num("input_thresh")?,
        target_offset: num("target_offset")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EBUR128: &str = "[Parsed_ebur128_0 @ 0x563d6f0f7d80] Summary:

  Integrated loudness:
    I:         -21.8 LUFS
    Threshold: -31.8 LUFS

  Loudness range:
    LRA:         0.0 LU
    Threshold: -41.8 LUFS
    LRA low:   -21.8 LUFS
    LRA high:  -21.8 LUFS

  True peak:
    Peak:      -16.3 dBFS
";

    #[test]
    fn reads_the_integrated_loudness_and_the_true_peak() {
        let l = parse_ebur128(EBUR128).unwrap();
        assert_eq!(l.loudness, Some(-21.8));
        assert_eq!(l.peak, Some(-16.3));
        let silent = EBUR128
            .replace(
                "-21.8 LUFS\n    Threshold: -31.8",
                "-70.0 LUFS\n    Threshold: -70.0",
            )
            .replace("-16.3 dBFS", "-inf dBFS");
        assert_eq!(parse_ebur128(&silent), Some(Level::default()));
        assert_eq!(parse_ebur128("no summary here"), None);
    }

    #[test]
    fn several_tracks_count_as_the_loudest() {
        let l = Level::loudest(&[
            Level {
                loudness: Some(-20.0),
                peak: Some(-6.0),
            },
            Level {
                loudness: Some(-14.0),
                peak: Some(-9.0),
            },
            Level::default(),
        ]);
        assert_eq!((l.loudness, l.peak), (Some(-14.0), Some(-6.0)));
        assert_eq!(Level::loudest(&[]), Level::default());
    }

    #[test]
    fn reads_the_first_pass_of_loudnorm() {
        let log = "[Parsed_loudnorm_0 @ 0x55] \n{\n\t\"input_i\" : \"-21.76\",\n\t\"input_tp\" : \"-16.32\",\n\t\"input_lra\" : \"0.10\",\n\t\"input_thresh\" : \"-31.76\",\n\t\"output_i\" : \"-16.01\",\n\t\"output_tp\" : \"-10.58\",\n\t\"output_lra\" : \"0.00\",\n\t\"output_thresh\" : \"-26.01\",\n\t\"normalization_type\" : \"linear\",\n\t\"target_offset\" : \"0.01\"\n}\n";
        let m = parse_loudnorm(log).unwrap();
        assert_eq!(m.input_i, -21.76);
        assert_eq!(m.input_thresh, -31.76);
        assert_eq!(m.target_offset, 0.01);
        assert_eq!(parse_loudnorm(&log.replace("\"-21.76\"", "\"-inf\"")), None);
    }
}
