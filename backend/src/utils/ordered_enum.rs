use enum_map::{EnumArray, EnumMap};
use enumset::{EnumSet, EnumSetType};
use serde::de::{Error, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::de::DeserializeAsWrap;
use serde_with::{DeserializeAs, Same, SerializeAs};
use std::fmt::{Display, Formatter};
use std::iter;
use std::marker::PhantomData;
use thiserror::Error;

#[derive(Debug)]
pub struct EnumOrderMap<T: EnumArray<Neighbors<T>>> {
    neighbors: EnumMap<T, Neighbors<T>>,
}

impl<T: EnumArray<Neighbors<T>>> EnumOrderMap<T> {
    pub fn new_unordered() -> Self {
        Self {
            neighbors: EnumMap::default(),
        }
    }

    pub fn group_starts(&self) -> impl Iterator<Item = T> {
        self.neighbors
            .iter()
            .filter(|(_, n)| matches!(n, Neighbors::Successor(_)))
            .map(|(v, _)| v)
    }

    pub fn neighbors_iter(&self) -> impl Iterator<Item = (T, &Neighbors<T>)> {
        self.neighbors.iter()
    }
}

impl<T: EnumArray<Neighbors<T>>> EnumOrderMap<T>
where
    T: Copy,
{
    pub fn add_order(&mut self, left: T, right: T) -> Result<(), OrderedEnumError<T>> {
        let new_left = match self.neighbors[left] {
            Neighbors::Isolated => Neighbors::Successor(right),
            Neighbors::Predecessor(pred) => Neighbors::Both(pred, right),
            Neighbors::Successor(succ) | Neighbors::Both(_, succ) => {
                return Err(OrderedEnumError::SuccessorAlreadyExists { value: left, succ });
            }
        };
        let new_right = match self.neighbors[right] {
            Neighbors::Isolated => Neighbors::Predecessor(left),
            Neighbors::Successor(succ) => Neighbors::Both(left, succ),
            Neighbors::Predecessor(pred) | Neighbors::Both(pred, _) => {
                return Err(OrderedEnumError::PredecessorAlreadyExists { value: right, pred });
            }
        };
        self.neighbors[left] = new_left;
        self.neighbors[right] = new_right;
        Ok(())
    }

    pub fn predecessor(&self, value: T) -> Option<T> {
        match self.neighbors[value] {
            Neighbors::Predecessor(pred) | Neighbors::Both(pred, _) => Some(pred),
            _ => None,
        }
    }

    pub fn successor(&self, value: T) -> Option<T> {
        match self.neighbors[value] {
            Neighbors::Successor(succ) | Neighbors::Both(_, succ) => Some(succ),
            _ => None,
        }
    }

    /// Returns all successors of `start`, in order, starting with `start` itself
    pub fn successors(&self, start: T) -> impl Iterator<Item = T> {
        iter::successors(Some(start), |v| self.successor(*v))
    }
}

impl<T: EnumArray<Neighbors<T>>> Copy for EnumOrderMap<T> where T::Array: Copy {}

impl<T: EnumArray<Neighbors<T>>> Clone for EnumOrderMap<T>
where
    T::Array: Clone,
{
    fn clone(&self) -> Self {
        Self {
            neighbors: self.neighbors.clone(),
        }
    }
}

impl<T: EnumArray<Neighbors<T>>> Default for EnumOrderMap<T> {
    fn default() -> Self {
        if T::LENGTH < 2 {
            return Self::new_unordered();
        }
        Self {
            neighbors: EnumMap::from_fn(|v: T| {
                let idx = v.into_usize();
                if idx == 0 {
                    Neighbors::Successor(T::from_usize(1))
                } else if idx == T::LENGTH - 1 {
                    Neighbors::Predecessor(T::from_usize(T::LENGTH - 2))
                } else {
                    Neighbors::Both(T::from_usize(idx - 1), T::from_usize(idx + 1))
                }
            }),
        }
    }
}

impl<T: EnumArray<Neighbors<T>>> Serialize for EnumOrderMap<T>
where
    T: Copy + Serialize + EnumSetType,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        EnumOrderMapAs::<Option<Same>>::serialize_as(self, serializer)
    }
}

impl<'de, T: EnumArray<Neighbors<T>>> Deserialize<'de> for EnumOrderMap<T>
where
    T: Copy + Deserialize<'de> + Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        EnumOrderMapAs::<Option<Same>>::deserialize_as(deserializer)
    }
}

pub struct EnumOrderMapAs<T>(PhantomData<T>);

impl<T: EnumArray<Neighbors<T>>, U> SerializeAs<EnumOrderMap<T>> for EnumOrderMapAs<U>
where
    T: Copy + Serialize + EnumSetType,
    U: SerializeAs<Option<T>>,
{
    fn serialize_as<S>(source: &EnumOrderMap<T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut output = vec![];
        let mut handled = EnumSet::new();
        for group_start in source.group_starts() {
            if !handled.is_empty() {
                output.push(None);
            }
            for element in source.successors(group_start) {
                handled.insert(element);
                output.push(Some(element));
            }
        }
        if handled.len() < T::LENGTH {
            for (key, neighbors) in source.neighbors_iter() {
                if handled.contains(key) || neighbors.is_isolated() {
                    continue;
                }
                if !handled.is_empty() {
                    output.push(None);
                }
                for element in source.successors(key) {
                    let was_new = handled.insert(element);
                    output.push(Some(element));
                    if !was_new {
                        break;
                    }
                }
            }
        }
        Vec::<U>::serialize_as(&output, serializer)
    }
}

impl<'de, T: EnumArray<Neighbors<T>>, U> DeserializeAs<'de, EnumOrderMap<T>> for EnumOrderMapAs<U>
where
    T: Copy + Deserialize<'de> + Display,
    U: DeserializeAs<'de, Option<T>>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<EnumOrderMap<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ValueVisitor<T, U>(PhantomData<(T, U)>);
        impl<
            'de,
            T: EnumArray<Neighbors<T>> + Copy + Deserialize<'de> + Display,
            U: DeserializeAs<'de, Option<T>>,
        > Visitor<'de> for ValueVisitor<T, U>
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
                while let Some(value) = seq
                    .next_element::<DeserializeAsWrap<Option<T>, U>>()?
                    .map(|v| v.into_inner())
                {
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
        deserializer.deserialize_seq(ValueVisitor::<T, U>(PhantomData))
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub enum Neighbors<T> {
    #[default]
    Isolated,
    Successor(T),
    Predecessor(T),
    Both(T, T),
}

impl<T> Neighbors<T> {
    pub fn is_isolated(&self) -> bool {
        matches!(self, Self::Isolated)
    }
}

impl<T> From<Neighbors<T>> for (Option<T>, Option<T>) {
    fn from(value: Neighbors<T>) -> Self {
        match value {
            Neighbors::Isolated => (None, None),
            Neighbors::Successor(succ) => (None, Some(succ)),
            Neighbors::Predecessor(pred) => (Some(pred), None),
            Neighbors::Both(pred, succ) => (Some(pred), Some(succ)),
        }
    }
}

impl<T> From<(Option<T>, Option<T>)> for Neighbors<T> {
    fn from(value: (Option<T>, Option<T>)) -> Self {
        match value {
            (None, None) => Neighbors::Isolated,
            (None, Some(succ)) => Neighbors::Successor(succ),
            (Some(pred), None) => Neighbors::Predecessor(pred),
            (Some(pred), Some(succ)) => Neighbors::Both(pred, succ),
        }
    }
}

#[derive(Error)]
pub enum OrderedEnumError<T> {
    #[error("{value} already has a successor: {succ}")]
    SuccessorAlreadyExists { value: T, succ: T },
    #[error("{value} already has a predecessor: {pred}")]
    PredecessorAlreadyExists { value: T, pred: T },
}
