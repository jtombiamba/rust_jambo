use uuid::Uuid;

/// Deterministically assign an entity id (game or room) to one of
/// `shard_count` shards. The hash is stable across processes so a given id
/// always lands on the same shard.
pub(crate) fn shard_for_id(id: Uuid, shard_count: usize) -> usize {
    let bytes = id.as_bytes();
    let hash = bytes
        .iter()
        .fold(0u64, |acc, &b| acc.wrapping_mul(31).wrapping_add(b as u64));
    (hash as usize) % shard_count
}

/// Extract a game id from a Redis channel name of the form "game:{uuid}".
pub(crate) fn extract_game_id_from_channel(channel: &str) -> Option<Uuid> {
    const PREFIX: &str = "game:";
    channel.strip_prefix(PREFIX).and_then(|s| s.parse().ok())
}

/// Extract a room id from a Redis channel name of the form "room:{uuid}".
pub(crate) fn extract_room_id_from_channel(channel: &str) -> Option<Uuid> {
    const PREFIX: &str = "room:";
    channel.strip_prefix(PREFIX).and_then(|s| s.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_for_id_is_deterministic_and_in_range() {
        let id = Uuid::new_v4();
        let shards = 8;
        let first = shard_for_id(id, shards);
        let second = shard_for_id(id, shards);
        assert_eq!(first, second);
        assert!(first < shards);
    }

    #[test]
    fn extract_game_id_from_channel_parses_valid_prefix() {
        let id = Uuid::new_v4();
        let channel = format!("game:{}", id);
        assert_eq!(extract_game_id_from_channel(&channel), Some(id));
    }

    #[test]
    fn extract_game_id_from_channel_rejects_other_prefixes() {
        let id = Uuid::new_v4();
        assert_eq!(extract_game_id_from_channel(&format!("room:{}", id)), None);
        assert_eq!(extract_game_id_from_channel("game:not-a-uuid"), None);
        assert_eq!(extract_game_id_from_channel("game:"), None);
    }

    #[test]
    fn extract_room_id_from_channel_parses_valid_prefix() {
        let id = Uuid::new_v4();
        let channel = format!("room:{}", id);
        assert_eq!(extract_room_id_from_channel(&channel), Some(id));
    }

    #[test]
    fn extract_room_id_from_channel_rejects_other_prefixes() {
        let id = Uuid::new_v4();
        assert_eq!(extract_room_id_from_channel(&format!("game:{}", id)), None);
        assert_eq!(extract_room_id_from_channel("room:not-a-uuid"), None);
    }
}
