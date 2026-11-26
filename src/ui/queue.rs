//! The play queue: its songs, the one playing, repeat, and the songs of a playlist
//! still coming in. Only the state: the app starts the songs these pick, prefetches,
//! and asks for radio.

use crate::model::Track;

#[derive(Clone, Copy, PartialEq)]
pub enum Repeat {
    Off,
    All,
    One,
}

/// A playlist's load whose next pages also go to the queue, and whether that queue
/// is shuffled (they're then mixed into what's still to come). `ran_out`: playback
/// reached the end of what had come, and carries on when more does (or the load ends).
pub struct QueueFill {
    pub id: String,
    pub load: u64,
    pub shuffled: bool,
    pub ran_out: bool,
}

/// More of a playlist joined the queue.
pub struct Filled {
    /// Playback was waiting for it: go on to the next song.
    pub ran_out: bool,
    /// The song playing was the last one before.
    pub was_at_end: bool,
}

pub struct Queue {
    pub tracks: Vec<Track>,
    /// The song playing (or paused, or loading).
    pub index: Option<usize>,
    pub repeat: Repeat,
    /// The queue is a playlist played while its songs were still coming in: the rest
    /// join the queue as they arrive.
    pub fill: Option<QueueFill>,
    /// IDs inserted by the user. Radio results are kept after these so manual
    /// additions always play first.
    manual: std::collections::HashSet<String>,
    /// Original order of automatic upcoming tracks while shuffle is active. Manual
    /// additions are kept ahead of these and are not part of the shuffled group.
    shuffle_order: Option<Vec<Track>>,
}

/// How many songs before and after the current one a saved session keeps: liked-songs
/// queues can run to 1000+ songs.
const SAVED_BEFORE: usize = 50;
const SAVED_AFTER: usize = 150;

impl Queue {
    pub fn new(repeat: Repeat) -> Self {
        Self {
            tracks: Vec::new(),
            index: None,
            repeat,
            fill: None,
            manual: std::collections::HashSet::new(),
            shuffle_order: None,
        }
    }

    pub fn current(&self) -> Option<&Track> {
        self.index.and_then(|i| self.tracks.get(i))
    }

    /// The song playing is the last one.
    pub fn at_end(&self) -> bool {
        self.index
            .is_some_and(|i| self.next_valid_after(i).is_none())
    }

    pub fn has_playable_after_current(&self) -> bool {
        self.index.is_some_and(|i| {
            self.next_valid_after(i).is_some()
                || self.fill.is_some()
                || (self.repeat == Repeat::All
                    && self
                        .tracks
                        .iter()
                        .enumerate()
                        .any(|(n, t)| n != i && Track::is_music(t)))
        })
    }

    pub fn halt(&mut self) {
        self.index = None;
    }

    pub fn select(&mut self, start: usize) -> bool {
        if let Some(i) = (start..self.tracks.len()).find(|&i| Track::is_music(&self.tracks[i])) {
            self.index = Some(i);
            true
        } else {
            false
        }
    }

    /// Play `tracks` from `start`; `fill`: a playlist still coming in, whose other songs
    /// follow. False (and nothing changes) when there are none.
    pub fn replace(&mut self, tracks: Vec<Track>, start: usize, fill: Option<QueueFill>) -> bool {
        let start = start.min(tracks.len().saturating_sub(1));
        let (tracks, Some(index)) = music_queue(tracks, start) else {
            return false;
        };
        self.adopt(tracks, index);
        self.fill = fill;
        true
    }

    /// Restore or adopt a queue without changing the caller's saved data.
    pub fn adopt(&mut self, tracks: Vec<Track>, index: usize) {
        self.manual.clear();
        self.shuffle_order = None;
        (self.tracks, self.index) = music_queue(tracks, index);
        self.fill = None;
    }

    /// Next, or the end of a song: true when there's a song to play. At the end of a
    /// playlist still coming in, playback waits for its next songs. (Repeating one song
    /// is the caller's: a song's end replays it rather than coming here.)
    pub fn advance(&mut self) -> bool {
        let Some(i) = self.index else { return false };
        if let Some(next) = self.next_valid_after(i) {
            self.index = Some(next);
        } else if let Some(fill) = &mut self.fill {
            fill.ran_out = true;
            return false;
        } else if self.repeat == Repeat::All
            && let Some(first) = self.tracks.iter().position(|track| Track::is_music(track))
        {
            self.index = Some(first);
        } else {
            return false;
        }
        true
    }

    /// Back to the song before: false at the first.
    pub fn back(&mut self) -> bool {
        match self.index.and_then(|i| {
            self.tracks[..i]
                .iter()
                .rposition(|track| Track::is_music(track))
        }) {
            Some(i) => {
                self.index = Some(i);
                true
            }
            None => false,
        }
    }

    /// What plays after the current song, to fetch ahead.
    pub fn upcoming(&self) -> Option<&Track> {
        let i = self.index?;
        match self.repeat {
            Repeat::One => None,
            Repeat::All => self
                .next_valid_after(i)
                .map(|n| &self.tracks[n])
                .or_else(|| self.tracks.iter().find(|track| Track::is_music(track))),
            Repeat::Off => self.next_valid_after(i).map(|n| &self.tracks[n]),
        }
    }

    fn next_valid_after(&self, i: usize) -> Option<usize> {
        (i + 1..self.tracks.len()).find(|&n| Track::is_music(&self.tracks[n]))
    }

    /// A song started: it no longer waits for a playlist's next songs.
    pub fn started(&mut self) {
        if let Some(fill) = &mut self.fill {
            fill.ran_out = false;
        }
    }

    /// Toggle shuffle for automatic songs still to come. Manual additions stay first;
    /// toggling off restores the prior automatic order, retaining new additions and
    /// respecting removals made while shuffled.
    pub fn shuffle_upcoming(&mut self) -> bool {
        let start = self.index.map_or(0, |i| i + 1);
        let automatic_indices: Vec<usize> = (start..self.tracks.len())
            .filter(|&i| {
                Track::is_music(&self.tracks[i]) && !self.manual.contains(&self.tracks[i].id)
            })
            .collect();
        if automatic_indices.is_empty() {
            return false;
        }

        if let Some(original) = self.shuffle_order.take() {
            let mut automatic: Vec<Track> = automatic_indices
                .iter()
                .map(|&i| self.tracks[i].clone())
                .collect();
            let mut restored = Vec::with_capacity(automatic.len());
            for track in original {
                if let Some(i) = automatic.iter().position(|queued| queued.id == track.id) {
                    restored.push(automatic.remove(i));
                }
            }
            restored.extend(automatic);
            for (i, track) in automatic_indices.into_iter().zip(restored) {
                self.tracks[i] = track;
            }
        } else {
            let original: Vec<Track> = automatic_indices
                .iter()
                .map(|&i| self.tracks[i].clone())
                .collect();
            if original.len() < 2 {
                return false;
            }
            let mut shuffled = original.clone();
            super::shuffle(&mut shuffled);
            for (i, track) in automatic_indices.into_iter().zip(shuffled) {
                self.tracks[i] = track;
            }
            self.shuffle_order = Some(original);
        }
        if let Some(fill) = &mut self.fill {
            fill.shuffled = self.shuffle_order.is_some();
        }
        true
    }

    /// Keep the source order behind a collection that was shuffled before it entered
    /// this queue, so the same shuffle control can restore that order later.
    pub fn remember_shuffle_order(&mut self, source_order: &[Track]) {
        let Some(current) = self.current().map(|track| track.id.as_str()) else {
            return;
        };
        let mut skipped_current = false;
        self.shuffle_order = Some(
            source_order
                .iter()
                .filter(|track| {
                    if !skipped_current && track.id == current {
                        skipped_current = true;
                        return false;
                    }
                    Track::is_music(track) && !self.manual.contains(&track.id)
                })
                .cloned()
                .collect(),
        );
    }

    /// Play `t` after the current song: true when nothing was playing, so it starts now.
    pub fn play_next(&mut self, t: Track) -> bool {
        if !Track::is_music(&t) {
            return false;
        }
        let at = self.index.map_or(0, |i| i + 1).min(self.tracks.len());
        self.manual.insert(t.id.clone());
        self.tracks.insert(at, t);
        if self.index.is_some() {
            return false;
        }
        self.index = Some(0);
        true
    }

    /// Add `t` at the end: true when nothing was playing, so it starts now.
    pub fn enqueue(&mut self, t: Track) -> bool {
        if !Track::is_music(&t) {
            return false;
        }
        self.manual.insert(t.id.clone());
        let at = self.index.map_or(0, |i| i + 1)
            + self
                .tracks
                .iter()
                .skip(self.index.map_or(0, |i| i + 1))
                .take_while(|queued| self.manual.contains(&queued.id))
                .count();
        self.tracks.insert(at, t);
        if self.index.is_some() {
            return false;
        }
        self.index = Some(at);
        true
    }

    /// Add a batch as manual tracks, preserving its order and putting it ahead of
    /// automatic recommendations. Existing and repeated IDs in the batch are skipped.
    pub fn enqueue_many(&mut self, tracks: Vec<Track>) -> (usize, bool) {
        let mut have: std::collections::HashSet<String> =
            self.tracks.iter().map(|track| track.id.clone()).collect();
        let mut added = 0;
        let mut starts = false;
        for track in tracks {
            if !Track::is_music(&track) || !have.insert(track.id.clone()) {
                continue;
            }
            starts |= self.enqueue(track);
            added += 1;
        }
        (added, starts)
    }

    /// Take song `i` out, unless it's the one playing.
    pub fn remove(&mut self, i: usize) {
        if i < self.tracks.len() && Some(i) != self.index {
            self.tracks.remove(i);
            if let Some(cur) = self.index.filter(|&c| c > i) {
                self.index = Some(cur - 1);
            }
        }
    }

    /// Slide queue row `i` one place — `dir` negative moves it up. Only what's still to
    /// come moves, and only past another song still to come: false otherwise.
    pub fn shift(&mut self, i: usize, dir: isize) -> bool {
        let first = self.index.map_or(0, |c| c + 1);
        let Some(j) = i.checked_add_signed(dir) else {
            return false;
        };
        if i < first || j < first || j >= self.tracks.len() {
            return false;
        }
        if !Track::is_music(&self.tracks[i]) || !Track::is_music(&self.tracks[j]) {
            return false;
        }
        self.tracks.swap(i, j);
        true
    }

    /// Radio songs for `seed` join the end, without the seed or songs already queued.
    /// Returns whether the song playing was the last one.
    pub fn add_radio(
        &mut self,
        seed: &str,
        tracks: Vec<Track>,
        recently_played: &std::collections::HashSet<String>,
    ) -> bool {
        let mut have: std::collections::HashSet<String> =
            self.tracks.iter().map(|t| t.id.clone()).collect();
        let was_at_end = self.at_end();
        let additions: Vec<_> = tracks
            .into_iter()
            .filter(|t| {
                t.is_music()
                    && t.id != seed
                    && !recently_played.contains(&t.id)
                    && have.insert(t.id.clone())
            })
            .collect();
        self.tracks.extend(additions);
        was_at_end
    }

    /// The next page of the playlist the queue fills from (`id`, `load`) joins it; `more`:
    /// pages still follow. `playing_it`: the queue still plays that playlist. `None` when
    /// the page doesn't apply (though the last one still ends the fill); an empty last
    /// page that does still says whether playback was waiting for it.
    pub fn fill_from(
        &mut self,
        id: &str,
        load: u64,
        tracks: Vec<Track>,
        more: bool,
        playing_it: bool,
    ) -> Option<Filled> {
        let fill = self
            .fill
            .as_mut()
            .filter(|f| f.id == id && f.load == load)?;
        let (shuffled, ran_out) = (fill.shuffled, std::mem::take(&mut fill.ran_out));
        if !more {
            self.fill = None;
        }
        if !playing_it {
            return None;
        }
        let was_at_end = self.at_end();
        self.tracks
            .extend(tracks.into_iter().filter(|t| t.is_music()));
        if shuffled {
            // Mixed into what's still to come, past the next song (it may be loading already).
            let from = self
                .index
                .map_or(0, |i| i + if was_at_end { 1 } else { 2 })
                .min(self.tracks.len());
            shuffle_music(&mut self.tracks[from..]);
        }
        Some(Filled {
            ran_out,
            was_at_end,
        })
    }

    pub fn reject_music_id(&mut self, id: &str) {
        let next = self.index.and_then(|i| {
            self.tracks
                .get(i..)?
                .iter()
                .any(|t| t.id != id)
                .then(|| self.tracks[..i].iter().filter(|t| t.id != id).count())
        });
        self.tracks.retain(|t| t.id != id);
        self.index = next;
        self.manual.remove(id);
        if let Some(order) = &mut self.shuffle_order {
            order.retain(|t| t.id != id);
        }
    }

    /// The part of the queue a saved session keeps, and the current song's place in it.
    pub fn saved(&self) -> Option<(&[Track], usize)> {
        let i = self.index?;
        let from = i.saturating_sub(SAVED_BEFORE);
        let to = (i + SAVED_AFTER).min(self.tracks.len());
        Some((&self.tracks[from..to], i - from))
    }
}

fn music_queue(tracks: Vec<Track>, start: usize) -> (Vec<Track>, Option<usize>) {
    let mut index = None;
    let mut music = Vec::new();
    for (i, track) in tracks.into_iter().enumerate() {
        if track.is_music() {
            if i >= start && index.is_none() {
                index = Some(music.len());
            }
            music.push(track);
        }
    }
    (music, index)
}

pub fn shuffle_music(tracks: &mut [Track]) {
    let indices: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter_map(|(i, track)| track.is_music().then_some(i))
        .collect();
    if indices.len() < 2 {
        return;
    }
    let mut shuffled: Vec<Track> = indices.iter().map(|&i| tracks[i].clone()).collect();
    super::shuffle(&mut shuffled);
    for (i, track) in indices.into_iter().zip(shuffled) {
        tracks[i] = track;
    }
}

#[cfg(test)]
mod tests {
    use super::{Queue, QueueFill, Repeat, Track};

    fn track(id: &str) -> Track {
        Track {
            id: id.into(),
            title: id.into(),
            artists: vec![],
            album: None,
            duration: None,
            thumbs: vec![],
            track_nr: None,
            plays: None,
            music_video: false,
            content_kind: crate::model::ContentKind::Unknown,
        }
    }

    fn video(id: &str) -> Track {
        let mut t = track(id);
        t.music_video = true;
        t
    }

    fn queue(songs: &[&str], index: usize, repeat: Repeat) -> Queue {
        let mut q = Queue::new(repeat);
        assert!(q.replace(songs.iter().map(|s| track(s)).collect(), index, None));
        q
    }

    fn ids(q: &Queue) -> Vec<&str> {
        q.tracks.iter().map(|t| t.id.as_str()).collect()
    }

    fn filling(id: &str, load: u64, shuffled: bool) -> Option<QueueFill> {
        Some(QueueFill {
            id: id.into(),
            load,
            shuffled,
            ran_out: false,
        })
    }

    #[test]
    fn next_and_back_follow_repeat() {
        let mut q = queue(&["a", "b"], 0, Repeat::Off);
        assert!(q.advance());
        assert_eq!(q.index, Some(1));
        // At the end: stays there, unless repeating all.
        assert!(!q.advance());
        assert_eq!(q.index, Some(1));
        q.repeat = Repeat::All;
        assert!(q.advance());
        assert_eq!(q.index, Some(0));
        // Next moves on even when repeating one song (the song's own end replays it).
        q.repeat = Repeat::One;
        assert!(q.advance());
        assert_eq!(q.index, Some(1));
        assert!(q.back());
        assert!(!q.back());
        assert_eq!(q.index, Some(0));
        // Nothing queued: nothing to do.
        assert!(!Queue::new(Repeat::All).advance());
    }

    #[test]
    fn back_tracks_play_order_across_radio_and_repeated_ids() {
        let mut q = queue(
            &["search-seed", "radio-a", "radio-a", "radio-b"],
            0,
            Repeat::Off,
        );
        assert!(q.advance());
        assert!(q.advance());
        assert_eq!(q.current().unwrap().id, "radio-a");
        assert!(q.back());
        assert_eq!(q.index, Some(1));
        assert!(q.back());
        assert_eq!(q.current().unwrap().id, "search-seed");
    }

    #[test]
    fn manual_additions_play_before_radio_and_next_advances_in_that_order() {
        let mut q = queue(&["search-seed"], 0, Repeat::Off);
        q.add_radio("search-seed", vec![track("radio-a")], &Default::default());
        q.enqueue(track("manual-a"));
        q.enqueue(track("manual-b"));
        assert_eq!(ids(&q), ["search-seed", "manual-a", "manual-b", "radio-a"]);
        assert!(q.advance());
        assert_eq!(q.current().unwrap().id, "manual-a");
        assert!(q.advance());
        assert_eq!(q.current().unwrap().id, "manual-b");
        assert!(q.advance());
        assert_eq!(q.current().unwrap().id, "radio-a");
    }

    #[test]
    fn filtered_queue_advances_and_rewinds_between_music_tracks() {
        let mut q = Queue::new(Repeat::Off);
        q.replace(
            vec![
                track("one"),
                video("legacy-video"),
                video("legacy-episode"),
                track("two"),
            ],
            0,
            None,
        );
        assert!(q.advance());
        assert_eq!(q.current().unwrap().id, "two");
        assert_eq!(ids(&q), ["one", "two"]);
        assert!(!q.advance());
        assert!(q.back());
        assert_eq!(q.current().unwrap().id, "one");
    }

    #[test]
    fn starting_on_invalid_playlist_entry_skips_to_next_music_track() {
        let mut q = Queue::new(Repeat::Off);
        assert!(q.replace(vec![video("old"), track("song")], 0, None));
        assert_eq!(q.current().unwrap().id, "song");
        assert_eq!(q.tracks.len(), 1);
    }

    #[test]
    fn restored_and_manual_queues_allow_verified_music_videos_only() {
        use crate::model::ContentKind;
        let mut episode = track("episode");
        episode.content_kind = ContentKind::PodcastEpisode;
        let mut live = video("live-performance");
        live.content_kind = ContentKind::MusicVideo;
        live.duration = Some(5400);
        let mut q = Queue::new(Repeat::All);
        q.adopt(vec![episode.clone(), live], 0);
        assert_eq!(ids(&q), ["live-performance"]);
        assert!(!q.play_next(episode.clone()));
        assert!(!q.enqueue(episode));
        assert_eq!(ids(&q), ["live-performance"]);
    }

    #[test]
    fn what_plays_next_is_fetched_ahead() {
        let q = queue(&["a", "b"], 1, Repeat::Off);
        assert!(q.upcoming().is_none());
        let q = queue(&["a", "b"], 1, Repeat::All);
        assert_eq!(q.upcoming().unwrap().id, "a");
        let q = queue(&["a", "b"], 0, Repeat::One);
        assert!(q.upcoming().is_none());
        let q = queue(&["a", "b"], 0, Repeat::Off);
        assert_eq!(q.upcoming().unwrap().id, "b");
    }

    #[test]
    fn playing_nothing_leaves_the_queue_as_it_was() {
        let mut q = Queue::new(Repeat::Off);
        q.replace(vec![track("a")], 0, filling("PL1", 1, false));
        assert!(!q.replace(vec![], 0, None));
        assert_eq!(ids(&q), ["a"]);
        assert_eq!(q.fill.as_ref().unwrap().id, "PL1");
        // A start past the end plays the last song.
        assert!(q.replace(vec![track("x"), track("y")], 9, None));
        assert_eq!(q.current().unwrap().id, "y");
    }

    #[test]
    fn songs_added_by_hand() {
        let mut q = Queue::new(Repeat::Off);
        // Into an empty queue: it starts.
        assert!(q.enqueue(track("a")));
        assert_eq!(q.index, Some(0));
        assert!(!q.enqueue(track("c")));
        assert!(!q.play_next(track("b")));
        assert_eq!(ids(&q), ["a", "b", "c"]);
        let mut empty = Queue::new(Repeat::Off);
        assert!(empty.play_next(track("x")));
        assert_eq!(empty.current().unwrap().id, "x");

        // Taking one out keeps the song playing where it is; the one playing stays.
        q.index = Some(2);
        q.remove(0);
        assert_eq!(ids(&q), ["b", "c"]);
        assert_eq!(q.current().unwrap().id, "c");
        q.remove(1);
        q.remove(5);
        assert_eq!(ids(&q), ["b", "c"]);
    }

    #[test]
    fn album_batch_enqueue_deduplicates_and_stays_ahead_of_radio() {
        let mut q = queue(&["now", "radio-a"], 0, Repeat::Off);
        let (added, starts) = q.enqueue_many(vec![
            track("album-a"),
            track("radio-a"),
            track("album-a"),
            track("album-b"),
        ]);
        assert_eq!(added, 2);
        assert!(!starts);
        assert_eq!(ids(&q), ["now", "album-a", "album-b", "radio-a"]);

        let mut empty = Queue::new(Repeat::Off);
        let (added, starts) = empty.enqueue_many(vec![track("one"), track("two")]);
        assert_eq!((added, starts), (2, true));
        assert_eq!(ids(&empty), ["one", "two"]);
    }

    #[test]
    fn album_batch_enqueue_rejects_non_music() {
        let mut episode = track("episode");
        episode.content_kind = crate::model::ContentKind::PodcastEpisode;
        let mut q = Queue::new(Repeat::Off);
        assert_eq!(q.enqueue_many(vec![track("song"), episode]), (1, true));
        assert_eq!(q.current().unwrap().id, "song");
    }

    #[test]
    fn a_queued_song_can_move_one_place() {
        let mut q = queue(&["now", "a", "b", "c"], 0, Repeat::Off);
        // The song playing and the one before it stay where they are.
        assert!(!q.shift(0, 1));
        assert!(!q.shift(1, -1));
        assert!(q.shift(1, 1));
        assert_eq!(ids(&q), ["now", "b", "a", "c"]);
        // The last row has nowhere to go down to.
        assert!(!q.shift(3, 1));
        assert!(q.shift(3, -1));
        assert_eq!(ids(&q), ["now", "b", "c", "a"]);
        // Nothing playing: every row is coming up, so the first may move down.
        let mut q = Queue::new(Repeat::Off);
        q.replace(vec![track("a"), track("b")], 0, None);
        q.index = None;
        assert!(q.shift(0, 1));
        assert_eq!(ids(&q), ["b", "a"]);
    }

    #[test]
    fn radio_adds_only_new_songs() {
        let mut q = queue(&["seed", "a"], 1, Repeat::Off);
        assert!(q.add_radio(
            "seed",
            vec![track("seed"), track("a"), track("b"), track("c")],
            &std::collections::HashSet::new()
        ));
        assert_eq!(ids(&q), ["seed", "a", "b", "c"]);
        assert!(!q.add_radio("seed", vec![track("d")], &std::collections::HashSet::new()));
    }

    #[test]
    fn radio_skips_recent_plays_but_keeps_queue_order_and_new_songs() {
        let mut q = queue(&["seed"], 0, Repeat::Off);
        let recent = ["played-recently".to_owned()].into_iter().collect();
        q.add_radio(
            "seed",
            vec![track("played-recently"), track("fresh"), track("fresh")],
            &recent,
        );
        assert_eq!(ids(&q), ["seed", "fresh"]);
    }

    #[test]
    fn manual_queue_stays_ahead_of_radio_recommendations() {
        let mut q = queue(&["seed"], 0, Repeat::Off);
        q.add_radio(
            "seed",
            vec![track("radio-a"), track("radio-b")],
            &Default::default(),
        );
        q.enqueue(track("manual"));
        assert_eq!(ids(&q), ["seed", "manual", "radio-a", "radio-b"]);
        q.add_radio("seed", vec![track("radio-c")], &Default::default());
        assert_eq!(ids(&q), ["seed", "manual", "radio-a", "radio-b", "radio-c"]);
    }

    #[test]
    fn shuffle_keeps_manual_tracks_first_and_can_restore_radio_order() {
        let mut q = queue(&["seed"], 0, Repeat::Off);
        q.add_radio(
            "seed",
            (0..40).map(|i| track(&format!("radio-{i}"))).collect(),
            &Default::default(),
        );
        q.enqueue(track("manual-a"));
        q.enqueue(track("manual-b"));
        let original_automatic: Vec<String> = ids(&q)[3..].iter().map(|id| (*id).into()).collect();

        assert!(q.shuffle_upcoming());
        assert_eq!(ids(&q)[..3], ["seed", "manual-a", "manual-b"]);
        assert_ne!(
            ids(&q)[3..],
            original_automatic
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
        assert!(q.shuffle_upcoming());
        assert_eq!(
            ids(&q)[3..],
            original_automatic
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn disabling_collection_shuffle_restores_original_remaining_order() {
        let mut q = queue(&["a", "c", "b", "d"], 0, Repeat::Off);
        let source_order = vec![track("a"), track("b"), track("c"), track("d")];
        q.remember_shuffle_order(&source_order);
        assert!(q.shuffle_upcoming());
        assert_eq!(ids(&q), ["a", "b", "c", "d"]);
    }

    #[test]
    fn shuffle_restore_keeps_new_recommendations_and_drops_removed_tracks() {
        let mut q = queue(&["seed", "a", "b", "c"], 0, Repeat::Off);
        let original: Vec<String> = ids(&q)[1..].iter().map(|id| (*id).into()).collect();
        assert!(q.shuffle_upcoming());
        let removed = q.tracks.iter().position(|t| t.id == "b").unwrap();
        q.remove(removed);
        q.add_radio("seed", vec![track("new")], &Default::default());
        assert!(q.shuffle_upcoming());
        assert_eq!(ids(&q), ["seed", "a", "c", "new"]);
        assert_eq!(
            original
                .into_iter()
                .filter(|id| id != "b")
                .collect::<Vec<_>>(),
            vec!["a", "c"]
        );
    }

    #[test]
    fn radio_and_shuffle_keep_only_eligible_tracks() {
        let mut q = Queue::new(Repeat::Off);
        q.replace(
            vec![track("now"), video("old"), track("a"), track("b")],
            0,
            None,
        );
        assert!(!q.add_radio(
            "now",
            vec![video("podcast"), track("radio-song")],
            &Default::default()
        ));
        assert_eq!(q.tracks.last().unwrap().id, "radio-song");
        assert!(q.shuffle_upcoming());
        assert!(q.tracks.iter().all(Track::is_music));
        assert!(q.advance());
        assert!(!q.current().unwrap().music_video);
    }

    #[test]
    fn a_playlist_still_coming_in_fills_the_queue() {
        let mut q = Queue::new(Repeat::Off);
        q.replace(vec![track("a"), track("b")], 0, filling("PL1", 3, false));
        // Another playlist's page, or another load's, doesn't join.
        assert!(
            q.fill_from("PL2", 3, vec![track("x")], true, true)
                .is_none()
        );
        assert!(
            q.fill_from("PL1", 2, vec![track("x")], true, true)
                .is_none()
        );
        let f = q
            .fill_from("PL1", 3, vec![video("podcast"), track("c")], true, true)
            .unwrap();
        assert!(!f.ran_out && !f.was_at_end);
        assert_eq!(ids(&q), ["a", "b", "c"]);

        // Playback reached the end before the next page: it waits, then carries on.
        q.index = Some(2);
        assert!(!q.advance());
        assert!(q.fill.as_ref().unwrap().ran_out);
        let f = q
            .fill_from("PL1", 3, vec![track("d")], false, true)
            .unwrap();
        assert!(f.ran_out && f.was_at_end);
        assert!(q.fill.is_none());
        assert!(q.advance());
        assert_eq!(q.current().unwrap().id, "d");
    }

    #[test]
    fn waiting_for_a_playlist_comes_before_repeating_it() {
        let mut q = Queue::new(Repeat::All);
        q.replace(vec![track("a"), track("b")], 1, filling("PL1", 1, false));
        // At the end of what came: it waits rather than starting over.
        assert!(!q.advance());
        assert_eq!(q.index, Some(1));
        // The load ends with nothing more: playback goes on, and now starts over.
        let f = q.fill_from("PL1", 1, vec![], false, true).unwrap();
        assert!(f.ran_out && f.was_at_end);
        assert!(q.fill.is_none());
        assert!(q.advance());
        assert_eq!(q.index, Some(0));
    }

    #[test]
    fn filling_stops_when_something_else_plays() {
        let mut q = Queue::new(Repeat::Off);
        q.replace(vec![track("a")], 0, filling("PL1", 1, false));
        // The queue plays something else now: the page doesn't join, and the last one ends the fill.
        assert!(
            q.fill_from("PL1", 1, vec![track("b")], true, false)
                .is_none()
        );
        assert!(q.fill.is_some());
        assert!(
            q.fill_from("PL1", 1, vec![track("c")], false, false)
                .is_none()
        );
        assert!(q.fill.is_none());
        assert_eq!(ids(&q), ["a"]);
        // A song starting means it no longer waits.
        q.replace(vec![track("a")], 0, filling("PL1", 2, false));
        q.advance();
        q.started();
        assert!(!q.fill.as_ref().unwrap().ran_out);
    }

    #[test]
    fn shuffling_up_next_leaves_what_played() {
        let songs: Vec<String> = (0..40).map(|i| i.to_string()).collect();
        let refs: Vec<&str> = songs.iter().map(String::as_str).collect();
        let mut q = queue(&refs, 5, Repeat::Off);
        q.fill = filling("PL1", 1, false);
        assert!(q.shuffle_upcoming());
        assert!(q.fill.as_ref().unwrap().shuffled);
        assert_eq!(ids(&q)[..6], refs[..6]);
        let mut rest: Vec<&str> = ids(&q)[6..].to_vec();
        // 34 songs coming back in their order by chance is too unlikely to happen.
        assert_ne!(rest, refs[6..]);
        rest.sort_by_key(|s| s.parse::<u32>().unwrap());
        assert_eq!(rest, refs[6..]);
        // At the last song there's nothing to shuffle.
        let mut q = queue(&["a", "b"], 1, Repeat::Off);
        assert!(!q.shuffle_upcoming());
    }

    #[test]
    fn a_shuffled_playlists_next_pages_mix_in_after_the_next_song() {
        let songs: Vec<String> = (0..40).map(|i| i.to_string()).collect();
        let refs: Vec<&str> = songs.iter().map(String::as_str).collect();
        // The song after the current one stays: it may be loading already.
        let mut q = queue(&refs[..10], 2, Repeat::Off);
        q.fill = filling("PL1", 1, true);
        let more: Vec<Track> = (10..40).map(|i| track(&i.to_string())).collect();
        q.fill_from("PL1", 1, more, true, true).unwrap();
        assert_eq!(ids(&q)[..4], refs[..4]);
        let mut rest: Vec<&str> = ids(&q)[4..].to_vec();
        assert_ne!(rest, refs[4..]);
        rest.sort_by_key(|s| s.parse::<u32>().unwrap());
        assert_eq!(rest, refs[4..]);
    }

    #[test]
    fn a_saved_session_keeps_the_songs_around_the_current_one() {
        let songs: Vec<String> = (0..300).map(|i| i.to_string()).collect();
        let refs: Vec<&str> = songs.iter().map(String::as_str).collect();
        let q = queue(&refs, 120, Repeat::Off);
        let (saved, i) = q.saved().unwrap();
        assert_eq!((saved.len(), i), (200, 50));
        assert_eq!(saved[i].id, "120");
        let q = queue(&refs[..10], 3, Repeat::Off);
        let (saved, i) = q.saved().unwrap();
        assert_eq!((saved.len(), i), (10, 3));
        assert!(Queue::new(Repeat::Off).saved().is_none());
    }
}
