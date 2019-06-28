use crate::run::{RequestResult, Seconds};
use serde::Serialize;
use std::collections::BTreeMap;

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
        for r in results {
            if let RequestResult::Ok { ref response, .. } = r {
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
    pub var: f64,
    pub sd: f64,
}
impl Latency {
    fn new(results: &[RequestResult]) -> Self {
        if results.is_empty() {
            return Latency::default();
        }
        let mut times = results.iter().map(|r| r.elapsed()).collect::<Vec<_>>();
        times.sort();

        let var = unbiased_variance(&times);
        Latency {
            min: times[0],
            median: times[times.len() / 2],
            mean: Seconds(times.iter().map(|t| t.0).sum::<f64>() / times.len() as f64),
            max: *times.last().unwrap(),
            var,
            sd: var.sqrt(),
        }
    }
}

fn unbiased_variance(samples: &[Seconds]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }

    let n = samples.len() as f64;
    let (sqsum, sum) = samples
        .iter()
        .fold((0.0, 0.0), |(a, b), s| (a + s.0 * s.0, b + s.0));
    let avg = sum / n;
    (sqsum - n * avg * avg) / (n - 1.0)
}

#[cfg(test)]
mod test {
    use super::*;
    use run::Seconds;

    #[test]
    fn var_works() {
        let samples = [0.7, -1.6, -0.2, -1.2, -0.1, 3.4, 3.7, 0.8, 0.0, 2.0]
            .iter()
            .cloned()
            .map(Seconds)
            .collect::<Vec<_>>();
        assert_eq!((unbiased_variance(&samples) * 10000.0).floor(), 32005.0);
    }
}
