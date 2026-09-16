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

/// A typed route parsed from a Redis channel name.
pub(crate) enum Channel {
    Game(Uuid),
    Room(Uuid),
    User(Uuid),
}

/// Parse a channel name of the form "{prefix}:{uuid}" into a typed route.
pub(crate) fn parse_channel(channel: &str) -> Option<Channel> {
    let (prefix, id) = channel.split_once(':')?;
    let id = id.parse::<Uuid>().ok()?;
    Some(match prefix {
        "game" => Channel::Game(id),
        "room" => Channel::Room(id),
        "user" => Channel::User(id),
        _ => return None,
    })
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
    fn parse_channel_game() {
        let id = Uuid::new_v4();
        let channel = format!("game:{}", id);
        assert!(matches!(parse_channel(&channel), Some(Channel::Game(g)) if g == id));
    }

    #[test]
    fn parse_channel_room() {
        let id = Uuid::new_v4();
        let channel = format!("room:{}", id);
        assert!(matches!(parse_channel(&channel), Some(Channel::Room(r)) if r == id));
    }

    #[test]
    fn parse_channel_user() {
        let id = Uuid::new_v4();
        let channel = format!("user:{}", id);
        assert!(matches!(parse_channel(&channel), Some(Channel::User(u)) if u == id));
    }

    #[test]
    fn parse_channel_rejects_unknown_prefix() {
        assert!(parse_channel("other:some-id").is_none());
    }

    #[test]
    fn parse_channel_rejects_malformed_id() {
        assert!(parse_channel("game:not-a-uuid").is_none());
    }

    #[test]
    fn parse_channel_rejects_missing_colon() {
        assert!(parse_channel("game").is_none());
        assert!(parse_channel("game:").is_none());
    }
}
