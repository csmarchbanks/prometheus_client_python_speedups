use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::{self, ErrorKind, Read},
    path::Path,
    str,
};

use pyo3::{exceptions::PyIOError, prelude::*, types::PyList};
use serde::{Deserialize, Serialize};

#[pyclass]
#[derive(Debug)]
pub struct Metric {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub documentation: String,
    #[pyo3(get)]
    pub typ: String,
    multiprocess_mode: Option<String>,
    #[pyo3(get)]
    pub samples: Vec<Sample>,
}

impl Metric {
    pub fn new(name: String, documentation: String, typ: String) -> Self {
        Self {
            name,
            documentation,
            typ,
            multiprocess_mode: None,
            samples: vec![],
        }
    }

    pub fn add_sample(
        &mut self,
        name: String,
        labels: BTreeMap<String, String>,
        value: f64,
        timestamp: f64,
    ) {
        self.samples.push(Sample {
            name,
            labels,
            value,
            timestamp,
        })
    }
}

#[pyclass]
#[derive(Clone, Debug)]
pub struct Sample {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub labels: BTreeMap<String, String>,
    #[pyo3(get)]
    pub timestamp: f64,
    #[pyo3(get)]
    pub value: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Key {
    metric_name: String,
    name: String,
    labels: BTreeMap<String, String>,
    help_text: String,
}

fn parse_key(key: &str) -> Key {
    let k: Key = serde_json::from_str(key).unwrap();
    k
}

#[derive(Debug)]
struct Value {
    key: String,
    timestamp: f64,
    value: f64,
}

fn read_all_values_from_file(path: &String) -> Result<Vec<Value>, io::Error> {
    let mut f = File::open(path)?;
    let initial_size: usize = 4096;
    let mut data = vec![0; initial_size];
    let n = f.read(&mut data)?;
    // We expect to at least be able to read how much of the file was used.
    if n < 4 {
        return Err(io::Error::from(ErrorKind::UnexpectedEof));
    } else if n < data.len() {
        data.truncate(n);
    }

    let used = u32::from_ne_bytes(data[0..4].try_into().unwrap()) as usize;
    // Just initialized but with no data yet, return early.
    if used == 0 {
        return Ok(vec![]);
    }
    if used > data.len() {
        data.resize(used, 0.try_into().unwrap());
        let n = f.read(&mut data[initial_size..])?;
        if n < used - initial_size {
            return Err(io::Error::from(ErrorKind::InvalidData));
        }
    }
    data.truncate(used);
    read_all_values(data, used)
}

fn read_all_values(data: Vec<u8>, used: usize) -> Result<Vec<Value>, io::Error> {
    let mut pos: usize = 8;
    let mut result: Vec<Value> = Vec::with_capacity(100);
    while pos < used {
        let encoded_len = u32::from_ne_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        if encoded_len + pos > used {
            return Err(io::Error::from(ErrorKind::InvalidData));
        }
        pos += 4;
        let encoded_key = &data[pos..pos + encoded_len];
        let padded_len = encoded_len + (8 - (encoded_len + 4) % 8);
        pos += padded_len;
        let value = f64::from_ne_bytes(data[pos..pos + 8].try_into().unwrap());
        let timestamp = f64::from_ne_bytes(data[pos + 8..pos + 16].try_into().unwrap());
        pos += 16;
        result.push(Value {
            key: str::from_utf8(encoded_key).unwrap().to_string(),
            timestamp,
            value,
        });
    }
    Ok(result)
}

/// Read metrics from all multiprocess files
fn read_multiprocess_files(files: &[String]) -> Result<HashMap<String, Metric>, PyErr> {
    let mut metrics: HashMap<String, Metric> = HashMap::new();
    let mut key_cache: HashMap<String, Key> = HashMap::new();

    for filepath in files.iter() {
        let parts: Vec<&str> = Path::new(filepath)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .split("_")
            .collect();

        let typ = parts[0];
        let is_live = typ == "gauge" && parts[1].starts_with("live");

        let values = match read_all_values_from_file(filepath) {
            Err(err) => {
                // Live gauges can be deleted between finding all .db files and reading them so
                // ignore NotFound errors in this case.
                if err.kind() == ErrorKind::NotFound && typ == "gauge" && is_live {
                    continue;
                }
                return Err(PyIOError::new_err(err.to_string()));
            }
            Ok(values) => values,
        };

        for value in values {
            let key = key_cache
                .entry(value.key)
                .or_insert_with_key(|key| parse_key(key));
            let metric = metrics.entry(key.metric_name.clone()).or_insert_with(|| {
                Metric::new(
                    key.metric_name.clone(),
                    key.help_text.clone(),
                    typ.to_string(),
                )
            });

            if typ == "gauge" {
                let pid = &parts[2][0..parts[2].len() - 3];
                metric.multiprocess_mode = Some(parts[1].to_string());
                key.labels.insert("pid".to_string(), pid.to_string());
                metric.add_sample(
                    key.name.clone(),
                    key.labels.clone(),
                    value.value,
                    value.timestamp,
                );
            } else {
                metric.add_sample(
                    key.name.clone(),
                    key.labels.clone(),
                    value.value,
                    value.timestamp,
                );
            }
        }
    }
    Ok(metrics)
}

fn accumulate_metrics(mut metrics: HashMap<String, Metric>) -> Vec<Metric> {
    for metric in metrics.values_mut() {
        let mut samples: HashMap<(String, BTreeMap<String, String>), f64> = HashMap::new();
        let mut sample_timestamps: HashMap<(String, BTreeMap<String, String>), f64> =
            HashMap::new();
        let mut buckets: HashMap<BTreeMap<String, String>, HashMap<String, f64>> = HashMap::new();
        for sample in &metric.samples {
            if metric.typ == "gauge" {
                let mut without_pid = sample.labels.clone();
                without_pid.remove("pid");
                let key_without_pid = (sample.name.clone(), without_pid);
                match metric.multiprocess_mode.as_deref() {
                    Some("min") | Some("livemin") => {
                        let current = samples.entry(key_without_pid).or_insert(sample.value);
                        if sample.value < *current {
                            *current = sample.value;
                        }
                    }
                    Some("max") | Some("livemax") => {
                        let current = samples.entry(key_without_pid).or_insert(sample.value);
                        if sample.value > *current {
                            *current = sample.value;
                        }
                    }
                    Some("sum") | Some("livesum") => {
                        samples
                            .entry(key_without_pid)
                            .and_modify(|current| *current += sample.value)
                            .or_insert(sample.value);
                    }
                    Some("mostrecent") | Some("livemostrecent") => {
                        match sample_timestamps.get_mut(&key_without_pid) {
                            Some(current_ts) => {
                                if sample.timestamp > *current_ts {
                                    samples.insert(key_without_pid, sample.value);
                                    *current_ts = sample.timestamp;
                                }
                            }
                            None => {
                                samples.insert(key_without_pid.clone(), sample.value);
                                sample_timestamps.insert(key_without_pid, sample.timestamp);
                            }
                        }
                    }
                    Some(_) | None => {
                        // all/liveall
                        samples.insert((sample.name.clone(), sample.labels.clone()), sample.value);
                    }
                };
            } else if metric.typ == "histogram" {
                match sample.labels.get("le") {
                    Some(le) => {
                        let mut without_le = sample.labels.clone();
                        without_le.remove("le");
                        let bucket = buckets.entry(without_le).or_default();
                        bucket
                            .entry(le.to_string())
                            .and_modify(|current| *current += sample.value)
                            .or_insert(sample.value);
                    }
                    None => {
                        samples
                            .entry((sample.name.clone(), sample.labels.clone()))
                            .and_modify(|current| *current += sample.value)
                            .or_insert(sample.value);
                    }
                }
            } else {
                samples
                    .entry((sample.name.clone(), sample.labels.clone()))
                    .and_modify(|current| *current += sample.value)
                    .or_insert(sample.value);
            }
        }
        // Accumulate bucket values
        if metric.typ == "histogram" {
            for (labels, values) in buckets.iter() {
                let mut acc = 0.0;
                let mut sorted: Vec<(&String, &f64)> = values.iter().collect();
                sorted.sort_by(|a, b| {
                    // Failure to unwrap would incidcate a corrupted file. Not much we could do
                    // about that.
                    let a_float: f64 = a.0.parse().unwrap();
                    let b_float: f64 = b.0.parse().unwrap();
                    a_float.total_cmp(&b_float)
                });
                for (bucket, value) in sorted {
                    let mut with_le = labels.clone();
                    with_le.insert("le".to_string(), (*bucket).clone());
                    let key = (metric.name.clone() + "_bucket", with_le);
                    acc += value;
                    samples.insert(key, acc);
                }
                let key = (metric.name.clone() + "_count", (*labels).clone());
                samples.insert(key, acc);
            }
        }

        metric.samples = samples
            .into_iter()
            .map(|((name, labels), value)| Sample {
                name,
                labels,
                value,
                timestamp: 0.0,
            })
            .collect();
    }
    metrics.into_values().collect()
}

pub fn merge_internal(files: &[String]) -> Result<Vec<Metric>, PyErr> {
    let metrics = read_multiprocess_files(files)?;
    Ok(accumulate_metrics(metrics))
}

#[pyfunction]
fn merge(files: &Bound<PyList>) -> PyResult<Vec<Metric>> {
    let filenames: Vec<String> = files.extract()?;
    merge_internal(&filenames)
}

/// A Python module implemented in Rust.
#[pymodule]
fn prometheus_client_python_speedups(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(merge, m)?)?;
    m.add_class::<Metric>()?;
    m.add_class::<Sample>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    #[test]
    fn test_merge() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("tests/dbfiles");

        println!("{:?}", d);
        let files: Vec<String> = fs::read_dir(d)
            .unwrap()
            .filter_map(|res| res.ok())
            .map(|dir| dir.path())
            .filter(|f| f.extension().map_or(false, |ext| ext == "db"))
            .map(|f| f.display().to_string())
            .collect();

        let result = merge_internal(&files);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }
}
