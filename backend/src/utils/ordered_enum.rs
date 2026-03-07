use enum_map::{EnumArray, EnumMap};
use serde::de::{Error, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::iter;
use std::marker::PhantomData;
use thiserror::Error;

#[derive(Debug)]
pub struct EnumOrderMap<T: EnumArray<Neighbor<T>>> {
    neighbors: EnumMap<T, Neighbor<T>>,
}

impl<T: EnumArray<Neighbor<T>>> EnumOrderMap<T> {
    pub fn new_unordered() -> Self {
        Self {
            neighbors: EnumMap::default(),
        }
    }

    pub fn group_starts(&self) -> impl Iterator<Item = T> {
        self.neighbors
            .iter()
            .filter(|(_, n)| matches!(n, Neighbor::Successor(_)))
            .map(|(v, _)| v)
    }
}

impl<T: EnumArray<Neighbor<T>>> EnumOrderMap<T>
where
    T: Copy,
{
    pub fn add_order(&mut self, left: T, right: T) -> Result<(), OrderedEnumError<T>> {
        let new_left = match self.neighbors[left] {
            Neighbor::Isolated => Neighbor::Successor(right),
            Neighbor::Predecessor(pred) => Neighbor::Both(pred, right),
            Neighbor::Successor(succ) | Neighbor::Both(_, succ) => {
                return Err(OrderedEnumError::SuccessorAlreadyExists { value: left, succ });
            }
        };
        let new_right = match self.neighbors[right] {
            Neighbor::Isolated => Neighbor::Predecessor(left),
            Neighbor::Successor(succ) => Neighbor::Both(left, succ),
            Neighbor::Predecessor(pred) | Neighbor::Both(pred, _) => {
                return Err(OrderedEnumError::PredecessorAlreadyExists { value: right, pred });
            }
        };
        self.neighbors[left] = new_left;
        self.neighbors[right] = new_right;
        Ok(())
    }

    pub fn predecessor(&self, value: T) -> Option<T> {
        match self.neighbors[value] {
            Neighbor::Predecessor(pred) | Neighbor::Both(pred, _) => Some(pred),
            _ => None,
        }
    }

    pub fn successor(&self, value: T) -> Option<T> {
        match self.neighbors[value] {
            Neighbor::Successor(succ) | Neighbor::Both(_, succ) => Some(succ),
            _ => None,
        }
    }

    /// Returns all successors of `start`, in order, starting with `start` itself
    pub fn successors(&self, start: T) -> impl Iterator<Item = T> {
        iter::successors(Some(start), |v| self.successor(*v))
    }
}

impl<T: EnumArray<Neighbor<T>>> Copy for EnumOrderMap<T> where T::Array: Copy {}

impl<T: EnumArray<Neighbor<T>>> Clone for EnumOrderMap<T>
where
    T::Array: Clone,
{
    fn clone(&self) -> Self {
        Self {
            neighbors: self.neighbors.clone(),
        }
    }
}

impl<T: EnumArray<Neighbor<T>>> Default for EnumOrderMap<T> {
    fn default() -> Self {
        if T::LENGTH < 2 {
            return Self::new_unordered();
        }
        Self {
            neighbors: EnumMap::from_fn(|v: T| {
                let idx = v.into_usize();
                if idx == 0 {
                    Neighbor::Successor(T::from_usize(1))
                } else if idx == T::LENGTH - 1 {
                    Neighbor::Predecessor(T::from_usize(T::LENGTH - 2))
                } else {
                    Neighbor::Both(T::from_usize(idx - 1), T::from_usize(idx + 1))
                }
            }),
        }
    }
}

impl<T: EnumArray<Neighbor<T>>> Serialize for EnumOrderMap<T>
where
    T: Copy + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(None)?;
        let mut has_any = false;
        for group_start in self.group_starts() {
            if has_any {
                seq.serialize_element(&Option::<T>::None)?;
            }
            has_any = true;
            for element in self.successors(group_start) {
                seq.serialize_element(&Some(element))?;
            }
        }
        seq.end()
    }
}

impl<'de, T: EnumArray<Neighbor<T>>> Deserialize<'de> for EnumOrderMap<T>
where
    T: Copy + Deserialize<'de> + Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ValueVisitor<T>(PhantomData<T>);
        impl<'de, T: EnumArray<Neighbor<T>> + Copy + Deserialize<'de> + Display> Visitor<'de>
            for ValueVisitor<T>
        {
            type Value = EnumOrderMap<T>;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence with enum values")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut result = EnumOrderMap::new_unordered();
                let mut current_pred = None;
                while let Some(value) = seq.next_element::<Option<T>>()? {
                    if let Some(pred) = current_pred
                        && let Some(succ) = value
                        && let Err(err) = result.add_order(pred, succ)
                    {
                        return Err(Error::custom(err));
                    }
                    current_pred = value;
                }
                Ok(result)
            }
        }
        deserializer.deserialize_seq(ValueVisitor(PhantomData))
    }
}

#[derive(Copy, Clone, Debug)]
#[doc(hidden)]
#[derive(Default)]
pub enum Neighbor<T> {
    #[default]
    Isolated,
    Successor(T),
    Predecessor(T),
    Both(T, T),
}

#[derive(Error)]
pub enum OrderedEnumError<T> {
    #[error("{value} already has a successor: {succ}")]
    SuccessorAlreadyExists { value: T, succ: T },
    #[error("{value} already has a predecessor: {pred}")]
    PredecessorAlreadyExists { value: T, pred: T },
}
