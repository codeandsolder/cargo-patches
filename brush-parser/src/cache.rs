//! Small shared success-only cache for parser entry points.
use lru::LruCache;
use std::{
    hash::Hash,
    num::NonZeroUsize,
    sync::{LazyLock, Mutex, PoisonError},
};

pub(crate) type ParseCache<K, V> = LazyLock<Mutex<LruCache<K, V>>>;

pub(crate) fn new<K: Hash + Eq, V>() -> Mutex<LruCache<K, V>> {
    Mutex::new(LruCache::new(NonZeroUsize::new(64).unwrap()))
}

pub(crate) fn parse<K: Hash + Eq, V: Clone, E>(
    cache: &ParseCache<K, V>,
    key: K,
    compute: impl FnOnce(&K) -> Result<V, E>,
) -> Result<V, E> {
    {
        let mut entries = cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(value) = entries.get(&key) {
            return Ok(value.clone());
        }
    }
    // Parsing can recursively invoke another parser. Do not retain the cache lock
    // while computing the result, and do not cache failures.
    let value = compute(&key)?;
    cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .put(key, value.clone());
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn caches_successes_not_errors_and_respects_lru_capacity() {
        let cache: ParseCache<u32, u32> = LazyLock::new(new);
        let calls = Cell::new(0);
        let compute = |key: &u32| {
            calls.set(calls.get() + 1);
            if *key == 0 { Err(()) } else { Ok(*key) }
        };
        assert_eq!(parse(&cache, 0, compute), Err(()));
        assert_eq!(parse(&cache, 0, compute), Err(()));
        assert_eq!(parse(&cache, 1, compute), Ok(1));
        assert_eq!(parse(&cache, 1, compute), Ok(1));
        assert_eq!(calls.get(), 3);
        for key in 2..=64 {
            assert_eq!(parse(&cache, key, compute), Ok(key));
        }
        assert_eq!(parse(&cache, 1, compute), Ok(1)); // refresh recency
        assert_eq!(parse(&cache, 65, compute), Ok(65));
        let before = calls.get();
        assert_eq!(parse(&cache, 1, compute), Ok(1));
        assert_eq!(calls.get(), before);
        assert_eq!(parse(&cache, 2, compute), Ok(2));
        assert_eq!(calls.get(), before + 1);
    }

    #[test]
    fn cache_does_not_hold_lock_while_computing() {
        let cache: ParseCache<u32, u32> = LazyLock::new(new);
        let value: Result<u32, ()> = parse(&cache, 1, |_| {
            let inner = parse(&cache, 2, |_| Ok(2))?;
            Ok(inner + 1)
        });
        assert_eq!(value, Ok(3));
    }
}
