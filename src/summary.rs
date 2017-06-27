use std::collections::BTreeMap;

use run::{RequestResult, Seconds};

#[derive(Debug, Serialize)]
pub struct Summary {
    pub count: Count,
    pub status: BTreeMap<u16, usize>,
    pub duration: Seconds,
    pub rps: f64,
    pub latency: Latency,
}
impl Summary {
    pub fn new(results: Vec<RequestResult>) -> Self {
        let count = Count::new(&results);
        let duration = results.iter().map(|r| r.end_time()).max().unwrap();
        let latency = Latency::new(&results);
        let mut status = BTreeMap::new();
        for r in results.iter() {
            if let RequestResult::Ok { ref response, .. } = *r {
                *status.entry(response.status).or_insert(0) += 1;
            }
        }
        Summary {
            count,
            status,
            rps: count.total as f64 / duration.0,
            duration,
            latency,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Count {
    pub total: usize,
    pub ok: usize,
    pub error: usize,
}
impl Count {
    fn new(results: &[RequestResult]) -> Self {
        let ok = results.iter().filter(|r| r.is_ok()).count();
        Count {
            total: results.len(),
            ok,
            error: results.len() - ok,
        }
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Latency {
    pub min: Seconds,
    pub median: Seconds,
    pub mean: Seconds,
    pub max: Seconds,
}
impl Latency {
    fn new(results: &[RequestResult]) -> Self {
        if results.is_empty() {
            return Latency::default();
        }
        let mut times = results.iter().map(|r| r.elapsed()).collect::<Vec<_>>();
        times.sort();
        Latency {
            min: times[0],
            median: times[times.len() / 2],
            mean: Seconds(times.iter().map(|t| t.0).sum::<f64>() / times.len() as f64),
            max: *times.last().unwrap(),
        }
    }
}
