use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::borrow::Borrow;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PrefixTree<K, V> {
    children: Vec<(K, Self)>,
    value: Option<V>,
}

impl<K, V> Default for PrefixTree<K, V> {
    fn default() -> Self {
        Self {
            children: vec![],
            value: None,
        }
    }
}

impl<K, V> PrefixTree<K, V> {
    pub fn value(&self) -> Option<&V> {
        self.value.as_ref()
    }
}

impl<K, V> PrefixTree<K, V>
where
    K: Ord,
{
    pub fn child<KB>(&self, k: KB) -> Option<&Self>
    where
        KB: Borrow<K>,
    {
        self.children
            .binary_search_by_key(&k.borrow(), |(key, _)| key)
            .ok()
            .map(|idx| &self.children[idx].1)
    }

    #[cfg(test)]
    pub fn indirect_child<KB, I>(&self, k: I) -> Option<&Self>
    where
        KB: Borrow<K>,
        I: IntoIterator<Item = KB>,
    {
        let mut tree = self;
        for part in k {
            tree = tree.child(part)?;
        }
        Some(tree)
    }

    #[cfg(test)]
    pub fn get<KB, I>(&self, k: I) -> Option<&V>
    where
        KB: Borrow<K>,
        I: IntoIterator<Item = KB>,
    {
        self.indirect_child(k)?.value.as_ref()
    }
}

impl<K, KI, V> FromIterator<(KI, V)> for PrefixTree<K, V>
where
    K: Ord,
    KI: IntoIterator<Item = K>,
{
    fn from_iter<T: IntoIterator<Item = (KI, V)>>(iter: T) -> Self {
        let iter = iter
            .into_iter()
            .map(|(sub, value)| (sub.into_iter().collect_vec(), value))
            .sorted_unstable_by(|(k1, _), (k2, _)| k1.cmp(k2));
        let mut stack = vec![(None, PrefixTree::default())];

        let finish_one = |stack: &mut Vec<(Option<K>, Self)>| {
            let (key, mut finished) = stack.pop().unwrap();
            finished.children.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
            finished.children.shrink_to_fit();
            stack
                .last_mut()
                .unwrap()
                .1
                .children
                .push((key.unwrap(), finished));
        };

        for (key, value) in iter {
            while stack.len() > key.len() + 1 {
                finish_one(&mut stack);
            }
            while stack.len() > 1
                && stack.last().unwrap().0.as_ref().unwrap() != &key[stack.len() - 2]
            {
                finish_one(&mut stack);
            }
            if stack.len() < key.len() + 1 {
                stack.extend(
                    key.into_iter()
                        .skip(stack.len() - 1)
                        .map(|k| (Some(k), PrefixTree::default())),
                );
            }
            stack.last_mut().unwrap().1.value = Some(value);
        }

        while stack.len() > 1 {
            finish_one(&mut stack);
        }
        let mut result = stack.into_iter().next().unwrap().1;
        result.children.sort_by(|(k1, _), (k2, _)| k1.cmp(k2));
        result.children.shrink_to_fit();
        result
    }
}

impl<I, K, KI, V> From<I> for PrefixTree<K, V>
where
    I: IntoIterator<Item = (KI, V)>,
    K: Ord,
    KI: IntoIterator<Item = K>,
{
    fn from(value: I) -> Self {
        Self::from_iter(value)
    }
}

#[cfg(test)]
mod test {
    use crate::utils::prefix_tree::PrefixTree;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_construct_tree() {
        let tree = PrefixTree::from([
            (vec!["hello", "world"], 1),
            (vec!["hello", "echo", "apple"], 2),
            (vec!["hello", "echo"], 3),
            (vec!["hello"], 4),
            (vec!["hello", "deeper", "key"], 5),
            (vec!["other"], 6),
            (vec!["some", "really", "long"], 7),
            (vec![], 8),
        ]);
        assert_eq!(tree.get(["hello", "world"]), Some(&1));
        assert_eq!(tree.get(["hello", "echo", "apple"]), Some(&2));
        assert_eq!(tree.get(["hello", "echo"]), Some(&3));
        assert_eq!(tree.get(["hello"]), Some(&4));
        assert_eq!(tree.get(["hello", "deeper", "key"]), Some(&5));
        assert_eq!(tree.get(["other"]), Some(&6));
        assert_eq!(tree.get(["some", "really", "long"]), Some(&7));
        assert_eq!(tree.get::<&str, _>([]), Some(&8));
        assert_eq!(tree.get(["none"]), None);
        assert_eq!(tree.get(["hello", "none"]), None);
    }
}
