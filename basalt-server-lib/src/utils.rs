use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub fn utc_now() -> DateTime<Utc> {
    chrono::offset::Local::now().to_utc()
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        match self {
            OneOrMany::One(_) => 1,
            OneOrMany::Many(items) => items.len(),
        }
    }
}

impl<T> IntoIterator for OneOrMany<T> {
    type Item = T;

    type IntoIter = OneOrManyIterator<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            OneOrMany::One(t) => OneOrManyIterator::One(t),
            OneOrMany::Many(t) => OneOrManyIterator::Many(t.into()), // Vec -> VecDeque is O(1) and does not realloc
        }
    }
}

impl<T> From<Vec<T>> for OneOrMany<T> {
    fn from(mut value: Vec<T>) -> Self {
        match value.len() {
            1 => OneOrMany::One(value.pop().unwrap()),
            _ => OneOrMany::Many(value),
        }
    }
}

pub enum OneOrManyIterator<T> {
    Empty,
    One(T),
    Many(VecDeque<T>),
}
impl<T> Iterator for OneOrManyIterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let mut this = Self::Empty;
        std::mem::swap(self, &mut this);
        match this {
            OneOrManyIterator::Empty => None,
            OneOrManyIterator::One(t) => {
                *self = Self::Empty;
                Some(t)
            }
            OneOrManyIterator::Many(mut v) => {
                if let Some(t) = v.pop_front() {
                    *self = Self::Many(v);
                    Some(t)
                } else {
                    *self = Self::Empty;
                    None
                }
            }
        }
    }
}
