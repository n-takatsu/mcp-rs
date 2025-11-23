//! Sorted Set (ZSET) operations for Redis
//! Provides ZADD, ZREM, ZRANGE, ZRANK, ZCOUNT, and related operations

use super::types::{RedisValue, SortedSetMember};
use crate::handlers::database::types::DatabaseError;
use std::collections::BTreeMap;

/// Sorted Set operations handler
pub struct SortedSetOperations;

impl SortedSetOperations {
    /// ZADD - Add members with scores to sorted set
    /// Returns number of elements added
    pub fn zadd(
        zset: &mut BTreeMap<String, f64>,
        members: Vec<(f64, String)>,
    ) -> Result<u32, DatabaseError> {
        let mut added = 0;
        for (score, member) in members {
            if !zset.contains_key(&member) {
                added += 1;
            }
            zset.insert(member, score);
        }
        Ok(added)
    }

    /// ZREM - Remove members from sorted set
    /// Returns number of elements removed
    pub fn zrem(
        zset: &mut BTreeMap<String, f64>,
        members: Vec<String>,
    ) -> Result<u32, DatabaseError> {
        let mut removed = 0;
        for member in members {
            if zset.remove(&member).is_some() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// ZRANGE - Get range of members by index
    pub fn zrange(
        zset: &BTreeMap<String, f64>,
        start: i32,
        stop: i32,
    ) -> Result<Vec<String>, DatabaseError> {
        let members: Vec<_> = zset.iter().collect();
        let len = members.len() as i32;

        // Normalize indices
        let start = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;
        let stop = if stop < 0 {
            (len + stop).max(-1)
        } else {
            stop.min(len - 1)
        } as usize;

        if start > stop || start >= members.len() {
            return Ok(vec![]);
        }

        Ok(members[start..=stop.min(members.len() - 1)]
            .iter()
            .map(|(k, _)| k.to_string())
            .collect())
    }

    /// ZRANGE with scores
    pub fn zrange_with_scores(
        zset: &BTreeMap<String, f64>,
        start: i32,
        stop: i32,
    ) -> Result<Vec<(String, f64)>, DatabaseError> {
        let members: Vec<_> = zset.iter().collect();
        let len = members.len() as i32;

        let start = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;
        let stop = if stop < 0 {
            (len + stop).max(-1)
        } else {
            stop.min(len - 1)
        } as usize;

        if start > stop || start >= members.len() {
            return Ok(vec![]);
        }

        Ok(members[start..=stop.min(members.len() - 1)]
            .iter()
            .map(|(k, v)| (k.to_string(), **v))
            .collect())
    }

    /// ZRANGEBYSCORE - Get members within score range
    pub fn zrange_by_score(
        zset: &BTreeMap<String, f64>,
        min: f64,
        max: f64,
    ) -> Result<Vec<String>, DatabaseError> {
        Ok(zset
            .iter()
            .filter(|(_, score)| **score >= min && **score <= max)
            .map(|(member, _)| member.clone())
            .collect())
    }

    /// ZRANK - Get rank (index) of member
    pub fn zrank(zset: &BTreeMap<String, f64>, member: &str) -> Result<Option<u32>, DatabaseError> {
        let members: Vec<_> = zset.iter().collect();
        Ok(members
            .iter()
            .position(|(k, _)| k.as_str() == member)
            .map(|pos| pos as u32))
    }

    /// ZSCORE - Get score of member
    pub fn zscore(
        zset: &BTreeMap<String, f64>,
        member: &str,
    ) -> Result<Option<f64>, DatabaseError> {
        Ok(zset.get(member).copied())
    }

    /// ZCARD - Get cardinality (number of members)
    pub fn zcard(zset: &BTreeMap<String, f64>) -> Result<u32, DatabaseError> {
        Ok(zset.len() as u32)
    }

    /// ZCOUNT - Count members within score range
    pub fn zcount(zset: &BTreeMap<String, f64>, min: f64, max: f64) -> Result<u32, DatabaseError> {
        Ok(zset
            .iter()
            .filter(|(_, score)| **score >= min && **score <= max)
            .count() as u32)
    }

    /// ZINCRBY - Increment score of member
    pub fn zincrby(
        zset: &mut BTreeMap<String, f64>,
        member: String,
        increment: f64,
    ) -> Result<f64, DatabaseError> {
        let new_score = zset.get(&member).unwrap_or(&0.0) + increment;
        zset.insert(member, new_score);
        Ok(new_score)
    }

    /// ZREMRANGEBYRANK - Remove members by rank range
    pub fn zrem_range_by_rank(
        zset: &mut BTreeMap<String, f64>,
        start: i32,
        stop: i32,
    ) -> Result<u32, DatabaseError> {
        let members: Vec<_> = zset.iter().map(|(k, _)| k.clone()).collect();
        let len = members.len() as i32;

        let start = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;
        let stop = if stop < 0 {
            (len + stop).max(-1)
        } else {
            stop.min(len - 1)
        } as usize;

        let mut removed = 0;
        if start <= stop && start < members.len() {
            for member in &members[start..=stop.min(members.len() - 1)] {
                if zset.remove(member).is_some() {
                    removed += 1;
                }
            }
        }

        Ok(removed)
    }

    /// ZREMRANGEBYSCORE - Remove members by score range
    pub fn zrem_range_by_score(
        zset: &mut BTreeMap<String, f64>,
        min: f64,
        max: f64,
    ) -> Result<u32, DatabaseError> {
        let members_to_remove: Vec<_> = zset
            .iter()
            .filter(|(_, score)| **score >= min && **score <= max)
            .map(|(k, _)| k.clone())
            .collect();

        let count = members_to_remove.len() as u32;
        for member in members_to_remove {
            zset.remove(&member);
        }

        Ok(count)
    }

    /// ZREVRANGE - Get range in reverse order
    pub fn zrevrange(
        zset: &BTreeMap<String, f64>,
        start: i32,
        stop: i32,
    ) -> Result<Vec<String>, DatabaseError> {
        let mut members: Vec<_> = zset.iter().collect();
        members.reverse();

        let len = members.len() as i32;
        let start = if start < 0 {
            (len + start).max(0)
        } else {
            start.min(len)
        } as usize;
        let stop = if stop < 0 {
            (len + stop).max(-1)
        } else {
            stop.min(len - 1)
        } as usize;

        if start > stop || start >= members.len() {
            return Ok(vec![]);
        }

        Ok(members[start..=stop.min(members.len() - 1)]
            .iter()
            .map(|(k, _)| k.to_string())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zadd() {
        let mut zset = BTreeMap::new();
        let result = SortedSetOperations::zadd(
            &mut zset,
            vec![(1.0, "one".to_string()), (2.0, "two".to_string())],
        );
        assert_eq!(result.unwrap(), 2);
        assert_eq!(zset.len(), 2);
    }

    #[test]
    fn test_zrem() {
        let mut zset = BTreeMap::new();
        SortedSetOperations::zadd(
            &mut zset,
            vec![(1.0, "one".to_string()), (2.0, "two".to_string())],
        )
        .unwrap();
        let result = SortedSetOperations::zrem(&mut zset, vec!["one".to_string()]);
        assert_eq!(result.unwrap(), 1);
        assert_eq!(zset.len(), 1);
    }

    #[test]
    fn test_zrange() {
        let mut zset = BTreeMap::new();
        SortedSetOperations::zadd(
            &mut zset,
            vec![
                (1.0, "one".to_string()),
                (2.0, "two".to_string()),
                (3.0, "three".to_string()),
            ],
        )
        .unwrap();
        let result = SortedSetOperations::zrange(&zset, 0, 1).unwrap();
        assert_eq!(result, vec!["one", "two"]);
    }

    #[test]
    fn test_zrank() {
        let mut zset = BTreeMap::new();
        SortedSetOperations::zadd(
            &mut zset,
            vec![
                (1.0, "one".to_string()),
                (2.0, "two".to_string()),
                (3.0, "three".to_string()),
            ],
        )
        .unwrap();
        let rank = SortedSetOperations::zrank(&zset, "two").unwrap();
        assert_eq!(rank, Some(1));
    }

    #[test]
    fn test_zrange_by_score() {
        let mut zset = BTreeMap::new();
        SortedSetOperations::zadd(
            &mut zset,
            vec![
                (1.0, "one".to_string()),
                (2.0, "two".to_string()),
                (3.0, "three".to_string()),
            ],
        )
        .unwrap();
        let result = SortedSetOperations::zrange_by_score(&zset, 1.5, 2.5).unwrap();
        assert_eq!(result, vec!["two"]);
    }

    #[test]
    fn test_zcard() {
        let mut zset = BTreeMap::new();
        SortedSetOperations::zadd(
            &mut zset,
            vec![(1.0, "one".to_string()), (2.0, "two".to_string())],
        )
        .unwrap();
        let count = SortedSetOperations::zcard(&zset).unwrap();
        assert_eq!(count, 2);
    }
}
