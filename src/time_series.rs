use std::collections::BTreeMap;

use run::{RequestResult, Seconds};

#[derive(Debug, Serialize)]
pub struct TimeSeries(Vec<Item>);
impl TimeSeries {
    pub fn new(results: Vec<RequestResult>) -> Self {
        let mut items = BTreeMap::new();
        let mut latencies = BTreeMap::new();
        for r in results {
            let time = r.start_time().0 as usize;
            let item = items.entry(time).or_insert_with(|| Item {
                time,
                ..Item::default()
            });
            item.requests += 1;

            latencies
                .entry(time)
                .or_insert_with(Vec::new)
                .push(r.elapsed().0);
        }
        TimeSeries(
            items
                .into_iter()
                .map(|(time, mut item)| {
                    let latencies = latencies.get_mut(&time).unwrap();
                    latencies.sort_by_key(|l| Seconds(*l));
                    item.latency.min = latencies[0];
                    item.latency.median = latencies[latencies.len() / 2];
                    item.latency.mean = latencies.iter().sum::<f64>() / latencies.len() as f64;
                    item.latency.max = *latencies.last().unwrap();
                    item
                })
                .collect(),
        )
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Item {
    pub time: usize, // seconds
    pub requests: usize,
    pub latency: Latency,
}

#[derive(Debug, Default, Serialize)]
pub struct Latency {
    pub min: f64,
    pub mean: f64,
    pub median: f64,
    pub max: f64,
}
