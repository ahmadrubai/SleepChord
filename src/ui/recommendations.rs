//! Lightweight, private ranking of provider chart songs using the local play history.

use std::collections::{HashMap, HashSet};

use crate::model::Track;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stage {
    Popular,
    Blending,
    Personalized,
}

#[derive(Clone)]
pub(super) struct Picks {
    pub tracks: Vec<Track>,
    pub stage: Stage,
}

impl super::App {
    pub(super) fn refresh_recommendations(&mut self) {
        let Some(chart) = self.explore.as_ref().map(|e| e.top.clone()) else {
            self.recommendations = None;
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.recommendations = Some(std::sync::Arc::new(rank(
            &chart,
            self.history.as_deref().unwrap_or_default(),
            &self.history_played_at,
            &self.history_play_counts,
            &self.liked,
            now,
        )));
    }
}

/// Rank the provider's current chart candidates locally. The provider chart remains the
/// cold-start source; no listening history leaves the device.
pub(super) fn rank(
    chart: &[Track],
    history: &[Track],
    played_at: &HashMap<String, u64>,
    play_counts: &HashMap<String, u32>,
    liked: &HashSet<String>,
    now: u64,
) -> Picks {
    let history: Vec<_> = history.iter().filter(|t| is_music(t)).collect();
    let total: u64 = history
        .iter()
        .map(|t| u64::from(play_counts.get(&t.id).copied().unwrap_or(1)))
        .sum();
    let stage = if history.is_empty() {
        Stage::Popular
    } else if history.len() >= 8 && total >= 24 {
        Stage::Personalized
    } else {
        Stage::Blending
    };

    let mut artist_affinity = HashMap::<String, f64>::new();
    let mut album_affinity = HashMap::<String, f64>::new();
    let mut max_affinity: f64 = 0.0;
    for track in &history {
        let count = f64::from(play_counts.get(&track.id).copied().unwrap_or(1).max(1));
        let age_days =
            now.saturating_sub(played_at.get(&track.id).copied().unwrap_or(now)) as f64 / 86_400.0;
        // A 30 day half-life rewards repeat listening without making one play decisive.
        let weight = count.sqrt() * 0.5_f64.powf(age_days / 30.0);
        for artist in &track.artists {
            let key = artist.id.as_deref().unwrap_or(&artist.name).to_lowercase();
            *artist_affinity.entry(key).or_default() += weight;
        }
        if let Some(album) = &track.album {
            let key = album.id.as_deref().unwrap_or(&album.name).to_lowercase();
            *album_affinity.entry(key).or_default() += weight * 0.8;
        }
        max_affinity = max_affinity.max(weight);
    }
    let liked_artists: HashSet<String> = history
        .iter()
        .filter(|track| liked.contains(&track.id))
        .flat_map(|track| track.artists.iter())
        .map(|artist| artist.id.as_deref().unwrap_or(&artist.name).to_lowercase())
        .collect();
    max_affinity = artist_affinity
        .values()
        .chain(album_affinity.values())
        .copied()
        .fold(max_affinity, f64::max)
        .max(1.0);

    let recent: HashSet<&str> = history.iter().take(8).map(|t| t.id.as_str()).collect();
    let mut seen = HashSet::new();
    let mut scored = Vec::new();
    for (rank, track) in chart.iter().enumerate() {
        if !is_music(track) || !seen.insert(track.id.as_str()) || recent.contains(track.id.as_str())
        {
            continue;
        }
        let artist_score = track
            .artists
            .iter()
            .map(|artist| {
                let key = artist.id.as_deref().unwrap_or(&artist.name).to_lowercase();
                artist_affinity.get(&key).copied().unwrap_or_default()
            })
            .fold(0.0, f64::max)
            / max_affinity;
        let album_score = track.album.as_ref().map_or(0.0, |album| {
            let key = album.id.as_deref().unwrap_or(&album.name).to_lowercase();
            album_affinity.get(&key).copied().unwrap_or_default() / max_affinity
        });
        let track_score = if history.iter().any(|h| h.id == track.id) {
            0.0
        } else {
            1.0
        };
        let liked_score = if liked.contains(&track.id)
            || track
                .artists
                .iter()
                .any(|a| liked_artists.contains(&a.id.as_deref().unwrap_or(&a.name).to_lowercase()))
        {
            1.0
        } else {
            0.0
        };
        let personal = (artist_score * 0.65 + album_score * 0.15 + liked_score * 0.2)
            .max(track_score * artist_score * 0.1);
        let confidence = total as f64 / (total as f64 + 12.0);
        let personal_mix = if history.is_empty() {
            0.0
        } else {
            confidence * 0.8
        };
        let chart_score = 1.0 / (rank as f64 + 2.0);
        let score = chart_score * (1.0 - personal_mix) + personal * personal_mix;
        scored.push((score, rank, track.clone()));
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    // Greedy artist cap: retain ranking while keeping one artist from dominating the row.
    let mut picked = Vec::with_capacity(scored.len());
    let mut deferred = Vec::new();
    let mut artist_counts = HashMap::<String, usize>::new();
    for item in scored {
        let artist = item
            .2
            .artists
            .first()
            .map(|a| a.id.as_deref().unwrap_or(&a.name).to_lowercase())
            .unwrap_or_default();
        let count = artist_counts.get(&artist).copied().unwrap_or_default();
        if count >= 2 {
            deferred.push(item.2);
        } else {
            *artist_counts.entry(artist).or_default() += 1;
            picked.push(item.2);
        }
    }
    picked.extend(deferred);
    if picked.is_empty() {
        let mut seen = HashSet::new();
        picked.extend(
            chart
                .iter()
                .filter(|track| is_music(track) && seen.insert(track.id.as_str()))
                .cloned(),
        );
    }
    Picks {
        tracks: picked,
        stage,
    }
}

fn is_music(track: &Track) -> bool {
    track.is_music()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Link;

    fn track(id: &str, artist: &str) -> Track {
        Track {
            id: id.into(),
            title: id.into(),
            artists: vec![Link {
                name: artist.into(),
                id: Some(artist.into()),
            }],
            album: None,
            duration: Some(180),
            thumbs: Vec::new(),
            track_nr: None,
            plays: None,
            music_video: false,
            content_kind: crate::model::ContentKind::Song,
        }
    }

    #[test]
    fn empty_history_uses_provider_chart_order_without_personalization() {
        let chart = vec![track("one", "A"), track("two", "B")];
        let picks = rank(
            &chart,
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            10,
        );
        assert_eq!(picks.stage, Stage::Popular);
        assert_eq!(
            picks
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["one", "two"]
        );
    }

    #[test]
    fn repeat_listening_moves_matching_artists_up_and_recent_song_is_excluded() {
        let history = vec![track("played", "B"), track("played-again", "B")];
        let chart = vec![
            track("A-chart", "A"),
            track("B-chart", "B"),
            track("played", "B"),
        ];
        let counts = HashMap::from([("played".into(), 15), ("played-again".into(), 12)]);
        let times = HashMap::from([("played".into(), 100), ("played-again".into(), 100)]);
        let picks = rank(&chart, &history, &times, &counts, &HashSet::new(), 100);
        assert_eq!(picks.tracks[0].id, "B-chart");
        assert!(!picks.tracks.iter().any(|t| t.id == "played"));
    }

    #[test]
    fn deduplicates_and_filters_podcast_and_video_content() {
        let mut podcast = track("podcast", "A");
        podcast.content_kind = crate::model::ContentKind::PodcastEpisode;
        let mut video = track("video", "A");
        video.content_kind = crate::model::ContentKind::OrdinaryVideo;
        let chart = vec![track("song", "A"), track("song", "A"), podcast, video];
        let picks = rank(
            &chart,
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
            0,
        );
        assert_eq!(
            picks
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["song"]
        );
    }

    #[test]
    fn established_history_uses_personalized_stage() {
        let history: Vec<_> = (0..8)
            .map(|i| track(&format!("old-{i}"), "Listener"))
            .collect();
        let counts = history.iter().map(|t| (t.id.clone(), 3)).collect();
        let picks = rank(
            &[track("chart-a", "Listener"), track("chart-b", "Elsewhere")],
            &history,
            &HashMap::new(),
            &counts,
            &HashSet::new(),
            1_000,
        );
        assert_eq!(picks.stage, Stage::Personalized);
        assert_eq!(picks.tracks[0].id, "chart-a");
    }
}
