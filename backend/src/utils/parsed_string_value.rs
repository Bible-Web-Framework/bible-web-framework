use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::any::type_name;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct ParsedStringValue<T> {
    pub value: T,
    pub string: String,
}

impl<T> Display for ParsedStringValue<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.string, f)
    }
}

impl<T: FromStr> FromStr for ParsedStringValue<T> {
    type Err = T::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self {
            value: T::from_str(s)?,
            string: s.to_owned(),
        })
    }
}

impl<T, Rhs> PartialEq<ParsedStringValue<Rhs>> for ParsedStringValue<T>
where
    T: PartialEq<Rhs>,
{
    fn eq(&self, other: &ParsedStringValue<Rhs>) -> bool {
        self.value.eq(&other.value)
    }

    #[allow(clippy::partialeq_ne_impl)]
    fn ne(&self, other: &ParsedStringValue<Rhs>) -> bool {
        self.value.ne(&other.value)
    }
}

impl<T: Eq> Eq for ParsedStringValue<T> {}

impl<T: Hash> Hash for ParsedStringValue<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state)
    }
}

impl<T> Serialize for ParsedStringValue<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.string)
    }
}

impl<'de, T: FromStr> Deserialize<'de> for ParsedStringValue<T>
where
    T::Err: Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{Error, Unexpected, Visitor};
        struct Deserializer<T>(PhantomData<T>);
        impl<'de, T: FromStr> Visitor<'de> for Deserializer<T>
        where
            T::Err: Display,
        {
            type Value = ParsedStringValue<T>;

            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                formatter.write_fmt(format_args!("a {} string", type_name::<T>()))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(ParsedStringValue {
                    value: T::from_str(v).map_err(|err| {
                        Error::custom(format_args!(
                            "invalid value: {}: {}",
                            Unexpected::Str(v),
                            err
                        ))
                    })?,
                    string: v.to_owned(),
                })
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(ParsedStringValue {
                    value: T::from_str(&v).map_err(|err| {
                        Error::custom(format_args!(
                            "invalid value: {}: {}",
                            Unexpected::Str(&v),
                            err
                        ))
                    })?,
                    string: v,
                })
            }
        }
        deserializer.deserialize_string(Deserializer(PhantomData))
    }
}
