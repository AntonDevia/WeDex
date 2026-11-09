use crate::model::{normalize, Track};
use std::collections::{HashMap, HashSet};

/// Session randomness for music selection (never used for credentials).
pub struct Sampler(u64);
impl Sampler {
    pub fn fresh() -> Self {
        Self(u64::from_le_bytes(
            uuid::Uuid::new_v4().as_bytes()[..8].try_into().unwrap(),
        ))
    }
    #[cfg(test)]
    pub fn seeded(seed: u64) -> Self {
        Self(seed)
    }
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        (((z ^ (z >> 31)) >> 11) as f64 + 1.) / ((1_u64 << 53) as f64 + 1.)
    }
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = (self.unit() * (i + 1) as f64) as usize;
            items.swap(i, j.min(i));
        }
    }
    fn weighted(&mut self, track: &Track, discovery: f32, recent: &HashSet<String>) -> f64 {
        let relevance = if track.score.is_finite() {
            track.score.clamp(0., 1.)
        } else {
            0.
        };
        let exponent = 6. - 5.8 * discovery.clamp(0., 1.) as f64;
        let weight = (0.02 + relevance).powf(exponent)
            * if recent.contains(&track.identity()) {
                0.10
            } else {
                1.
            };
        -self.unit().ln() / weight
    }
    pub fn recommendations(
        &mut self,
        tracks: Vec<Track>,
        excluded: &HashSet<String>,
        recent: &HashSet<String>,
        discovery: f32,
    ) -> Vec<Track> {
        let mut seen = excluded.clone();
        let mut candidates = tracks
            .into_iter()
            .filter(|t| seen.insert(t.identity()))
            .map(|t| {
                let key = self.weighted(&t, discovery, recent);
                (t, key)
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut artists = HashMap::<String, usize>::new();
        let mut last = String::new();
        let mut out = Vec::new();
        while out.len() < 30 {
            let allowed = |t: &Track| artists.get(&normalize(&t.artist)).copied().unwrap_or(0) < 2;
            let index = candidates
                .iter()
                .position(|(t, _)| allowed(t) && normalize(&t.artist) != last)
                .or_else(|| candidates.iter().position(|(t, _)| allowed(t)));
            let Some(index) = index else {
                break;
            };
            let (track, _) = candidates.remove(index);
            last = normalize(&track.artist);
            *artists.entry(last.clone()).or_default() += 1;
            out.push(track);
        }
        out
    }
    pub fn seeds(&mut self, favorites: &[Track], recent: &[String]) -> Vec<Track> {
        let recent = recent.iter().cloned().collect();
        let mut ranked = favorites
            .iter()
            .cloned()
            .map(|mut t| {
                t.score = 0.5;
                let key = self.weighted(&t, 0.5, &recent);
                (t, key)
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut artists = HashSet::new();
        let mut seen = HashSet::new();
        let mut out = vec![];
        for (track, _) in &ranked {
            if artists.insert(normalize(&track.artist)) && seen.insert(track.identity()) {
                out.push(track.clone());
                if out.len() == 4 {
                    return out;
                }
            }
        }
        for (track, _) in ranked {
            if seen.insert(track.identity()) {
                out.push(track);
                if out.len() == 4 {
                    break;
                }
            }
        }
        out
    }
}
pub fn remember(history: &mut Vec<String>, items: impl IntoIterator<Item = String>, limit: usize) {
    for item in items {
        history.retain(|old| old != &item);
        history.push(item);
    }
    if history.len() > limit {
        history.drain(..history.len() - limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pool() -> Vec<Track> {
        (0..120)
            .map(|i| {
                let mut t = crate::model::tests::track(
                    &format!("Track {i}"),
                    &format!("Artist {}", i % 40),
                    200,
                );
                t.id = i.to_string();
                t.score = 0.4 + (i % 6) as f64 * 0.1;
                t
            })
            .collect()
    }
    #[test]
    fn fresh_runs_change_order_and_members_without_losing_constraints() {
        let tracks = pool();
        let excluded = HashSet::from([tracks[0].identity()]);
        let a =
            Sampler::seeded(1).recommendations(tracks.clone(), &excluded, &HashSet::new(), 0.45);
        let b = Sampler::seeded(2).recommendations(tracks, &excluded, &HashSet::new(), 0.45);
        let keys = |v: &Vec<Track>| v.iter().map(Track::identity).collect::<Vec<_>>();
        assert_ne!(keys(&a), keys(&b));
        assert_ne!(
            keys(&a).into_iter().collect::<HashSet<_>>(),
            keys(&b).into_iter().collect()
        );
        for run in [&a, &b] {
            assert_eq!(run.len(), 30);
            assert_eq!(keys(run).into_iter().collect::<HashSet<_>>().len(), 30);
            assert!(!run.iter().any(|t| excluded.contains(&t.identity())));
            let mut counts = HashMap::new();
            for t in run {
                *counts.entry(&t.artist).or_insert(0) += 1;
            }
            assert!(counts.values().all(|n| *n <= 2));
            assert!(run.windows(2).all(|p| p[0].artist != p[1].artist));
        }
    }
    #[test]
    fn discovery_character_changes_relevance_of_the_selected_music() {
        let mut close = 0.;
        let mut explore = 0.;
        for seed in 0..100 {
            for (mode, total) in [(0.25, &mut close), (0.8, &mut explore)] {
                let result = Sampler::seeded(seed).recommendations(
                    pool(),
                    &HashSet::new(),
                    &HashSet::new(),
                    mode,
                );
                *total += result.iter().take(10).map(|t| t.score).sum::<f64>();
            }
        }
        assert!(
            close / 1000. > explore / 1000. + 0.055,
            "close={}, exploration={}",
            close / 1000.,
            explore / 1000.
        );
    }
    #[test]
    fn recent_batches_are_penalized_but_can_repeat_when_pool_is_small() {
        let tracks = pool();
        let recent: HashSet<_> = tracks[..60].iter().map(Track::identity).collect();
        let mut ordinary = 0;
        let mut penalized = 0;
        for seed in 1..40 {
            ordinary += Sampler::seeded(seed)
                .recommendations(tracks.clone(), &HashSet::new(), &HashSet::new(), 0.45)
                .iter()
                .filter(|t| recent.contains(&t.identity()))
                .count();
            penalized += Sampler::seeded(seed)
                .recommendations(tracks.clone(), &HashSet::new(), &recent, 0.45)
                .iter()
                .filter(|t| recent.contains(&t.identity()))
                .count();
        }
        assert!(penalized < ordinary / 2);
        assert_eq!(
            Sampler::seeded(1)
                .recommendations(tracks[..2].to_vec(), &HashSet::new(), &recent, 0.45)
                .len(),
            2
        );
    }
    #[test]
    fn saved_seeds_are_sampled_across_the_entire_collection() {
        let tracks = pool();
        let a = Sampler::seeded(1).seeds(&tracks, &[]);
        let recent = a.iter().map(Track::identity).collect::<Vec<_>>();
        let b = Sampler::seeded(2).seeds(&tracks, &recent);
        assert_eq!(a.len(), 4);
        assert!(a
            .iter()
            .any(|t| !tracks[..4].iter().any(|old| old.identity() == t.identity())));
        assert_ne!(recent, b.iter().map(Track::identity).collect::<Vec<_>>());
        let single = Sampler::seeded(1).seeds(&tracks[..1], &recent);
        assert_eq!(single.len(), 1);
    }
}
