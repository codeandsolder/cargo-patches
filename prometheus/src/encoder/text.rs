// Copyright 2019 TiKV Project Authors. Licensed under Apache-2.0.

use std::borrow::Cow;
use std::io::{self, Write};

use crate::errors::Result;
use crate::histogram::BUCKET_LABEL;
use crate::proto::{self, MetricFamily, MetricType};
#[cfg(feature = "protobuf")]
use crate::proto_ext::MessageFieldExt;

use super::{check_metric_family, Encoder};

/// The text format of metric family.
pub const TEXT_FORMAT: &str = "text/plain; version=0.0.4";

const POSITIVE_INF: &str = "+Inf";
const QUANTILE: &str = "quantile";

/// An implementation of an [`Encoder`] that converts a [`MetricFamily`] proto message
/// into text format.
#[derive(Debug, Default)]
pub struct TextEncoder;

impl TextEncoder {
    /// Create a new text encoder.
    pub fn new() -> TextEncoder {
        TextEncoder
    }
    /// Appends metrics to a given `String` buffer.
    ///
    /// This is a convenience wrapper around `<TextEncoder as Encoder>::encode`.
    pub fn encode_utf8(&self, metric_families: &[MetricFamily], buf: &mut String) -> Result<()> {
        // Note: it's important to *not* re-validate UTF8-validity for the
        // entirety of `buf`. Otherwise, repeatedly appending metrics to the
        // same `buf` will lead to quadratic behavior. That's why we use
        // `WriteUtf8` abstraction to skip the validation.
        self.encode_impl(metric_families, &mut StringBuf(buf))?;
        Ok(())
    }
    /// Converts metrics to `String`.
    ///
    /// This is a convenience wrapper around `<TextEncoder as Encoder>::encode`.
    pub fn encode_to_string(&self, metric_families: &[MetricFamily]) -> Result<String> {
        let mut buf = String::new();
        self.encode_utf8(metric_families, &mut buf)?;
        Ok(buf)
    }

    fn encode_impl(
        &self,
        metric_families: &[MetricFamily],
        writer: &mut dyn WriteUtf8,
    ) -> Result<()> {
        for mf in metric_families {
            // Fail-fast checks.
            check_metric_family(mf)?;

            // Write `# HELP` header.
            let name = mf.name();
            let help = mf.help();
            if !help.is_empty() {
                writer.write_all("# HELP ")?;
                writer.write_all(name)?;
                writer.write_all(" ")?;
                writer.write_all(&escape_string(help, false))?;
                writer.write_all("\n")?;
            }

            // Write `# TYPE` header.
            let metric_type = mf.get_field_type();
            let lowercase_type = format!("{:?}", metric_type).to_lowercase();
            writer.write_all("# TYPE ")?;
            writer.write_all(name)?;
            writer.write_all(" ")?;
            writer.write_all(&lowercase_type)?;
            writer.write_all("\n")?;

            for m in mf.get_metric() {
                match metric_type {
                    MetricType::COUNTER => {
                        write_sample(writer, name, None, m, None, m.get_counter().get_value())?;
                    }
                    MetricType::GAUGE => {
                        write_sample(writer, name, None, m, None, m.get_gauge().get_value())?;
                    }
                    MetricType::HISTOGRAM => {
                        let h = m.get_histogram();

                        let mut inf_seen = false;
                        for b in h.get_bucket() {
                            let upper_bound = b.upper_bound();
                            write_sample(
                                writer,
                                name,
                                Some("_bucket"),
                                m,
                                Some((BUCKET_LABEL, &format_float(upper_bound))),
                                b.cumulative_count() as f64,
                            )?;
                            if upper_bound.is_sign_positive() && upper_bound.is_infinite() {
                                inf_seen = true;
                            }
                        }
                        if !inf_seen {
                            write_sample(
                                writer,
                                name,
                                Some("_bucket"),
                                m,
                                Some((BUCKET_LABEL, POSITIVE_INF)),
                                h.get_sample_count() as f64,
                            )?;
                        }

                        write_sample(writer, name, Some("_sum"), m, None, h.get_sample_sum())?;

                        write_sample(
                            writer,
                            name,
                            Some("_count"),
                            m,
                            None,
                            h.get_sample_count() as f64,
                        )?;
                    }
                    MetricType::SUMMARY => {
                        let s = m.get_summary();

                        for q in s.get_quantile().iter() {
                            write_sample(
                                writer,
                                name,
                                None,
                                m,
                                Some((QUANTILE, &format_float(q.quantile()))),
                                q.value(),
                            )?;
                        }

                        write_sample(writer, name, Some("_sum"), m, None, s.sample_sum())?;

                        write_sample(
                            writer,
                            name,
                            Some("_count"),
                            m,
                            None,
                            s.sample_count() as f64,
                        )?;
                    }
                    MetricType::UNTYPED => {
                        #[cfg(feature = "protobuf")]
                        let value = m.untyped.value();
                        #[cfg(not(feature = "protobuf"))]
                        let value = m.get_untyped().get_value();
                        write_sample(writer, name, None, m, None, value)?;
                    }
                }
            }
        }

        Ok(())
    }
}

impl Encoder for TextEncoder {
    fn encode<W: Write>(&self, metric_families: &[MetricFamily], writer: &mut W) -> Result<()> {
        self.encode_impl(metric_families, &mut *writer)
    }

    fn format_type(&self) -> &str {
        TEXT_FORMAT
    }
}

fn format_float(value: f64) -> Cow<'static, str> {
    if value == f64::INFINITY {
        Cow::Borrowed(POSITIVE_INF)
    } else if value == f64::NEG_INFINITY {
        Cow::Borrowed("-Inf")
    } else if value.is_nan() {
        Cow::Borrowed("NaN")
    } else {
        Cow::Owned(value.to_string())
    }
}

/// `write_sample` writes a single sample in text format to `writer`, given the
/// metric name, an optional metric name postfix, the metric proto message
/// itself, optionally an additional label name and value (use empty strings if
/// not required), and the value. The function returns the number of bytes
/// written and any error encountered.
fn write_sample(
    writer: &mut dyn WriteUtf8,
    name: &str,
    name_postfix: Option<&str>,
    mc: &proto::Metric,
    additional_label: Option<(&str, &str)>,
    value: f64,
) -> Result<()> {
    writer.write_all(name)?;
    if let Some(postfix) = name_postfix {
        writer.write_all(postfix)?;
    }

    label_pairs_to_text(mc.get_label(), additional_label, writer)?;

    writer.write_all(" ")?;
    writer.write_all(&format_float(value))?;

    let timestamp = mc.timestamp_ms();
    if timestamp != 0 {
        writer.write_all(" ")?;
        writer.write_all(&timestamp.to_string())?;
    }

    writer.write_all("\n")?;

    Ok(())
}

/// `label_pairs_to_text` converts a slice of `LabelPair` proto messages plus
/// the explicitly given additional label pair into text formatted as required
/// by the text format and writes it to `writer`. An empty slice in combination
/// with an empty string `additional_label_name` results in nothing being
/// written. Otherwise, the label pairs are written, escaped as required by the
/// text format, and enclosed in '{...}'. The function returns the number of
/// bytes written and any error encountered.
fn label_pairs_to_text(
    pairs: &[proto::LabelPair],
    additional_label: Option<(&str, &str)>,
    writer: &mut dyn WriteUtf8,
) -> Result<()> {
    if pairs.is_empty() && additional_label.is_none() {
        return Ok(());
    }

    let mut separator = "{";
    for lp in pairs {
        writer.write_all(separator)?;
        writer.write_all(lp.name())?;
        writer.write_all("=\"")?;
        writer.write_all(&escape_string(lp.value(), true))?;
        writer.write_all("\"")?;

        separator = ",";
    }

    if let Some((name, value)) = additional_label {
        writer.write_all(separator)?;
        writer.write_all(name)?;
        writer.write_all("=\"")?;
        writer.write_all(&escape_string(value, true))?;
        writer.write_all("\"")?;
    }

    writer.write_all("}")?;

    Ok(())
}

fn find_first_occurence(v: &str, include_double_quote: bool) -> Option<usize> {
    if include_double_quote {
        memchr::memchr3(b'\\', b'\n', b'\"', v.as_bytes())
    } else {
        memchr::memchr2(b'\\', b'\n', v.as_bytes())
    }
}

/// `escape_string` replaces `\` by `\\`, new line character by `\n`, and `"` by `\"` if
/// `include_double_quote` is true.
///
/// Implementation adapted from
/// https://lise-henry.github.io/articles/optimising_strings.html
fn escape_string(v: &str, include_double_quote: bool) -> Cow<'_, str> {
    let first_occurence = find_first_occurence(v, include_double_quote);

    if let Some(first) = first_occurence {
        let mut escaped = String::with_capacity(v.len() * 2);
        escaped.push_str(&v[0..first]);
        let remainder = v[first..].chars();

        for c in remainder {
            match c {
                '\\' | '\n' => {
                    escaped.extend(c.escape_default());
                }
                '"' if include_double_quote => {
                    escaped.extend(c.escape_default());
                }
                _ => {
                    escaped.push(c);
                }
            }
        }

        escaped.shrink_to_fit();
        escaped.into()
    } else {
        // The input string does not contain any characters that would need to
        // be escaped. Return it as it is.
        v.into()
    }
}

trait WriteUtf8 {
    fn write_all(&mut self, text: &str) -> io::Result<()>;
}

impl<W: Write> WriteUtf8 for W {
    fn write_all(&mut self, text: &str) -> io::Result<()> {
        Write::write_all(self, text.as_bytes())
    }
}

/// Coherence forbids to impl `WriteUtf8` directly on `String`, need this
/// wrapper as a work-around.
struct StringBuf<'a>(&'a mut String);

impl WriteUtf8 for StringBuf<'_> {
    fn write_all(&mut self, text: &str) -> io::Result<()> {
        self.0.push_str(text);
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::counter::Counter;
    use crate::gauge::Gauge;
    use crate::histogram::{Histogram, HistogramOpts};
    use crate::metrics::{Collector, Opts};

    #[test]
    fn test_text_encoder_untyped() {
        let mut label = proto::LabelPair::default();
        label.set_name("source".into());
        label.set_value("sensor".into());
        let mut metric = proto::Metric::from_label(vec![label]);
        let mut untyped = proto::Untyped::default();
        untyped.set_value(-7.25);
        #[cfg(feature = "protobuf")]
        {
            metric.untyped = Some(untyped).into();
        }
        #[cfg(not(feature = "protobuf"))]
        metric.set_untyped(untyped);
        metric.set_timestamp_ms(123);
        let mut family = proto::MetricFamily::default();
        family.set_name("legacy_value".into());
        family.set_help("Legacy sensor".into());
        family.set_field_type(MetricType::UNTYPED);
        family.set_metric(vec![metric]);
        assert_eq!(
            TextEncoder::new().encode_to_string(&[family]).unwrap(),
            "# HELP legacy_value Legacy sensor\n# TYPE legacy_value untyped\nlegacy_value{source=\"sensor\"} -7.25 123\n"
        );
    }

    #[test]
    fn test_explicit_infinity_bucket_and_special_values() {
        let mut bucket = proto::Bucket::default();
        bucket.set_upper_bound(f64::INFINITY);
        bucket.set_cumulative_count(2);
        let mut histogram = proto::Histogram::default();
        histogram.set_sample_count(2);
        histogram.set_sample_sum(f64::INFINITY);
        histogram.set_bucket(vec![bucket]);
        let mut metric = proto::Metric::default();
        metric.set_histogram(histogram);
        let mut family = proto::MetricFamily::default();
        family.set_name("latency".into());
        family.set_field_type(MetricType::HISTOGRAM);
        family.set_metric(vec![metric]);
        assert_eq!(
            TextEncoder::new().encode_to_string(&[family]).unwrap(),
            "# TYPE latency histogram\nlatency_bucket{le=\"+Inf\"} 2\nlatency_sum +Inf\nlatency_count 2\n"
        );
        let gauge = Gauge::new("special", "Special values").unwrap();
        for (value, repr) in [(f64::NEG_INFINITY, "-Inf"), (f64::NAN, "NaN")] {
            gauge.set(value);
            let text = TextEncoder::new().encode_to_string(&gauge.collect()).unwrap();
            assert!(text.ends_with(&format!("special {}\n", repr)));
        }
    }

    #[test]
    fn test_escape_string() {
        assert_eq!(r"\\", escape_string("\\", false));
        assert_eq!(r"a\\", escape_string("a\\", false));
        assert_eq!(r"\n", escape_string("\n", false));
        assert_eq!(r"a\n", escape_string("a\n", false));
        assert_eq!(r"\\n", escape_string("\\n", false));

        assert_eq!(r##"\\n\""##, escape_string("\\n\"", true));
        assert_eq!(r##"\\\n\""##, escape_string("\\\n\"", true));
        assert_eq!(r##"\\\\n\""##, escape_string("\\\\n\"", true));
        assert_eq!(r##"\"\\n\""##, escape_string("\"\\n\"", true));
    }

    #[test]
    fn test_text_encoder() {
        let counter_opts = Opts::new("test_counter", "test help")
            .const_label("a", "1")
            .const_label("b", "2");
        let counter = Counter::with_opts(counter_opts).unwrap();
        counter.inc();

        let mf = counter.collect();
        let mut writer = Vec::<u8>::new();
        let encoder = TextEncoder::new();
        let txt = encoder.encode(&mf, &mut writer);
        assert!(txt.is_ok());

        let counter_ans = r##"# HELP test_counter test help
# TYPE test_counter counter
test_counter{a="1",b="2"} 1
"##;
        assert_eq!(counter_ans.as_bytes(), writer.as_slice());

        let gauge_opts = Opts::new("test_gauge", "test help")
            .const_label("a", "1")
            .const_label("b", "2");
        let gauge = Gauge::with_opts(gauge_opts).unwrap();
        gauge.inc();
        gauge.set(42.0);

        let mf = gauge.collect();
        writer.clear();
        let txt = encoder.encode(&mf, &mut writer);
        assert!(txt.is_ok());

        let gauge_ans = r##"# HELP test_gauge test help
# TYPE test_gauge gauge
test_gauge{a="1",b="2"} 42
"##;
        assert_eq!(gauge_ans.as_bytes(), writer.as_slice());
    }

    #[test]
    fn test_text_encoder_histogram() {
        let opts = HistogramOpts::new("test_histogram", "test help").const_label("a", "1");
        let histogram = Histogram::with_opts(opts).unwrap();
        histogram.observe(0.25);

        let mf = histogram.collect();
        let mut writer = Vec::<u8>::new();
        let encoder = TextEncoder::new();
        let res = encoder.encode(&mf, &mut writer);
        assert!(res.is_ok());

        let ans = r##"# HELP test_histogram test help
# TYPE test_histogram histogram
test_histogram_bucket{a="1",le="0.005"} 0
test_histogram_bucket{a="1",le="0.01"} 0
test_histogram_bucket{a="1",le="0.025"} 0
test_histogram_bucket{a="1",le="0.05"} 0
test_histogram_bucket{a="1",le="0.1"} 0
test_histogram_bucket{a="1",le="0.25"} 1
test_histogram_bucket{a="1",le="0.5"} 1
test_histogram_bucket{a="1",le="1"} 1
test_histogram_bucket{a="1",le="2.5"} 1
test_histogram_bucket{a="1",le="5"} 1
test_histogram_bucket{a="1",le="10"} 1
test_histogram_bucket{a="1",le="+Inf"} 1
test_histogram_sum{a="1"} 0.25
test_histogram_count{a="1"} 1
"##;
        assert_eq!(ans.as_bytes(), writer.as_slice());
    }

    #[test]
    fn test_text_encoder_summary() {
        use crate::proto::{Metric, Quantile, Summary};
        use std::str;

        let mut metric_family = MetricFamily::default();
        metric_family.set_name("test_summary".to_string());
        metric_family.set_help("This is a test summary statistic".to_string());
        metric_family.set_field_type(MetricType::SUMMARY);

        let mut summary = Summary::default();
        summary.set_sample_count(5.0 as u64);
        summary.set_sample_sum(15.0);

        let mut quantile1 = Quantile::default();
        quantile1.set_quantile(50.0);
        quantile1.set_value(3.0);

        let mut quantile2 = Quantile::default();
        quantile2.set_quantile(100.0);
        quantile2.set_value(5.0);

        summary.set_quantile(vec![quantile1, quantile2]);

        let mut metric = Metric::default();
        metric.set_summary(summary);
        metric_family.set_metric(vec![metric]);

        let mut writer = Vec::<u8>::new();
        let encoder = TextEncoder::new();
        let res = encoder.encode(&vec![metric_family], &mut writer);
        assert!(res.is_ok());

        let ans = r##"# HELP test_summary This is a test summary statistic
# TYPE test_summary summary
test_summary{quantile="50"} 3
test_summary{quantile="100"} 5
test_summary_sum 15
test_summary_count 5
"##;
        assert_eq!(ans, str::from_utf8(writer.as_slice()).unwrap());
    }

    #[test]
    fn test_text_encoder_to_string() {
        let counter_opts = Opts::new("test_counter", "test help")
            .const_label("a", "1")
            .const_label("b", "2");
        let counter = Counter::with_opts(counter_opts).unwrap();
        counter.inc();

        let mf = counter.collect();

        let encoder = TextEncoder::new();
        let txt = encoder.encode_to_string(&mf);
        let txt = txt.unwrap();

        let counter_ans = r##"# HELP test_counter test help
# TYPE test_counter counter
test_counter{a="1",b="2"} 1
"##;
        assert_eq!(counter_ans, txt.as_str());
    }
}
