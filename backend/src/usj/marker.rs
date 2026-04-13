use serde_with::{DeserializeFromStr, SerializeDisplay};
use std::borrow::Cow;
use std::convert::identity;
use std::fmt::{Display, Formatter};
use std::num::{NonZeroU8, ParseIntError};
use std::str::FromStr;
use strum::IntoStaticStr;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum MarkerParseError {
    #[error("Unknown marker for location \\{0}")]
    UnknownMarker(String),
    #[error("Marker # bigger than {0}")]
    OutOfRange(u8),
    #[error("Invalid number: {0}")]
    InvalidNumber(#[from] ParseIntError),
}

pub trait MacroEnum: Copy {
    fn to_cow_str(self) -> Cow<'static, str>;
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, IntoStaticStr)]
pub enum MilestoneSide {
    #[strum(serialize = "-s")]
    Start,
    #[strum(serialize = "-e")]
    End,
}

macro_rules! marker_enum {
    ($name:ident => $(|)? $($marker:ident ($($marker_args:tt)*))|+) => {
        #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, SerializeDisplay, DeserializeFromStr)]
        pub enum $name {
            $($marker(marker_enum_args!($($marker_args)*)),)+
        }

        impl MacroEnum for $name {
            fn to_cow_str(self) -> Cow<'static, str> {
                match self {
                    $(Self::$marker(_) => marker_enum_cow_str!($marker, self, $($marker_args)*),)+
                }
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(Self::$marker(value) => {
                        f.write_str(identconv::lower_strify!($marker))?;
                        marker_enum_display!(f, value, $($marker_args)*);
                    })+
                }
                Ok(())
            }
        }

        impl FromStr for $name {
            type Err = MarkerParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $(marker_enum_from_str!(
                    s, $marker, identconv::lower_strify!($marker), identity, $($marker_args)*
                );)+
                Err(MarkerParseError::UnknownMarker(s.to_string()))
            }
        }
    };
}

macro_rules! marker_enum_args {
    () => {
        ()
    };

    (..=$num_end:literal) => {
        u8
    };

    (range) => {
        (u8, u8)
    };

    (side $(, $($arg:tt)+)?) => {
        (MilestoneSide, marker_enum_args!($($($arg)+)?))
    };
}

macro_rules! marker_enum_cow_str {
    ($marker:ident, $self:expr,) => {
        Cow::Borrowed(identconv::lower_strify!($marker))
    };

    ($marker:ident, $self:expr, $($_:tt)+) => {
        Cow::Owned($self.to_string())
    };
}

macro_rules! marker_enum_display {
    ($f:ident, $value:expr,) => {
        let _ = $value;
    };

    ($f:ident, $value:expr, ..=$num_end:literal) => {
        $f.write_fmt(format_args!("{}", $value))?;
    };

    ($f:ident, $value:expr, range) => {{
        let (start, end) = $value;
        $f.write_fmt(format_args!("{start}"))?;
        if start != end {
            $f.write_fmt(format_args!("-{end}"))?;
        }
    }};

    ($f:ident, $value:expr, side $(, $($arg:tt)+)?) => {{
        let (side, arg) = $value;
        marker_enum_display!($f, arg, $($($arg)+)?);
        $f.write_str(side.into())?;
    }};
}

macro_rules! marker_enum_from_str {
    ($s:expr, $marker:ident, $marker_str:expr, $adapter:expr,) => {
        if $s == $marker_str {
            return Ok(Self::$marker($adapter(())));
        }
    };

    ($s:expr, $marker:ident, $marker_str:expr, $adapter:expr, ..=$num_end:literal) => {
        if let Some(trimmed) = as_number_marker($s, $marker_str) {
            if trimmed.is_empty() {
                return Ok(Self::$marker($adapter(1)));
            }
            let value = trimmed.parse::<NonZeroU8>()?.get();
            if value > $num_end {
                return Err(MarkerParseError::OutOfRange($num_end));
            }
            return Ok(Self::$marker($adapter(value)));
        }
    };

    ($s:expr, $marker:ident, $marker_str:expr, $adapter:expr, range) => {
        if let Some(trimmed) = as_number_marker($s, $marker_str) {
            return Ok(Self::$marker($adapter(
                if trimmed.is_empty() {
                    (1, 1)
                } else if let Some((left, right)) = trimmed.split_once('-') {
                    (left.parse::<NonZeroU8>()?.get(), right.parse::<NonZeroU8>()?.get())
                } else {
                    let value = trimmed.parse::<NonZeroU8>()?.get();
                    (value, value)
                }
            )))
        }
    };

    ($s:expr, $marker:ident, $marker_str:expr, $adapter:expr, side $(, $($arg:tt)+)?) => {{
        if let Some(trimmed) = $s.strip_suffix("-s") {
            marker_enum_from_str!(
                trimmed,
                $marker,
                $marker_str,
                const { |v| (MilestoneSide::Start, $adapter(v)) },
                $($($arg)+)?
            );
        }
        if let Some(trimmed) = $s.strip_suffix("-e") {
            marker_enum_from_str!(
                trimmed,
                $marker,
                $marker_str,
                const { |v| (MilestoneSide::End, $adapter(v)) },
                $($($arg)+)?
            );
        }
    }};
}

fn as_number_marker<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.strip_prefix(prefix)
        .filter(|x| x.is_empty() || x.as_bytes()[0].is_ascii_digit())
}

marker_enum!(
    ContentMarker =>
        // para
        | Ide()
        | Sts()
        | Rem()
        | H()
        | Toc(..=3)
        | Toca(..=3)
        | Imt(..=4)
        | Is(..=2)
        | Ip()
        | Ipi()
        | Im()
        | Imi()
        | Ipq()
        | Imq()
        | Ipr()
        | Ipc()
        | Iq(..=3)
        | Ili(..=2)
        | Ib()
        | Iot()
        | Io(..=4)
        | Iex()
        | Imte()
        | Ie()
        | Mt(..=4)
        | Mte(..=2)
        | Cl()
        | Cd()
        | Ms(..=3)
        | Mr()
        | S(..=4)
        | Sr()
        | R()
        | D()
        | Sp()
        | Sd(..=4)
        | P()
        | M()
        | Po()
        | Cls()
        | Pr()
        | Pc()
        | Pm()
        | Pmo()
        | Pmc()
        | Pmr()
        | Pi(..=3)
        | Mi(..=3)
        | Lit()
        | Nb()
        | B()
        | Ph(..=3)
        | Q(..=4)
        | Qr()
        | Qc()
        | Qa()
        | Qm(..=3)
        | Qd()
        | Lh()
        | Li(..=4)
        | Lf()
        | Lim(..=4)
        | Tr()
        // char
        | Add()
        | Bk()
        | Dc()
        | Em()
        | Jmp()
        | K()
        | Nd()
        | Ord()
        | Pn()
        | Png()
        | Qt()
        | Rb()
        | Rq()
        | Ref()
        | Sig()
        | Sls()
        | Tl()
        | W()
        | Wa()
        | Wg()
        | Wh()
        | Wj()
        | Addpn()
        | Pro()
        | Bd()
        | It()
        | Bdit()
        | No()
        | Sc()
        | Sup()
        | Pb()
        | Ior()
        | Iqt()
        | Qac()
        | Qs()
        | Litl()
        | Lik()
        | Liv()
        | Fr()
        | Fq()
        | Fqa()
        | Fk()
        | Ft()
        | Fl()
        | Fw()
        | Fp()
        | Fv()
        | Fdc()
        | Fm()
        | Xo()
        | Xop()
        | Xk()
        | Xq()
        | Xt()
        | Xta()
        | Xot()
        | Xnt()
        | Xdc()
        // table:cell
        | Th(range)
        | Thr(range)
        | Thc(range)
        | Tc(range)
        | Tcr(range)
        | Tcc(range)
);

marker_enum!(MilestoneMarker => Qt(side, ..=5) | Ts(side));

marker_enum!(NoteMarker => F() | Fe() | Ef() | X() | Ex());

#[cfg(test)]
mod test {
    use crate::usj::marker::{ContentMarker, MarkerParseError, MilestoneMarker, MilestoneSide};
    use cool_asserts::assert_matches;
    use pretty_assertions::assert_eq;
    use std::num::IntErrorKind;
    use std::str::FromStr;

    #[test]
    fn test_success() {
        assert_eq!("p".parse(), Ok(ContentMarker::P(())));
        assert_eq!("pi".parse(), Ok(ContentMarker::Pi(1)));
        assert_eq!("pi1".parse(), Ok(ContentMarker::Pi(1)));
        assert_eq!("s".parse(), Ok(ContentMarker::S(1)));
        assert_eq!("s3".parse(), Ok(ContentMarker::S(3)));
        assert_eq!("sd".parse(), Ok(ContentMarker::Sd(1)));
        assert_eq!("sd4".parse(), Ok(ContentMarker::Sd(4)));
        assert_eq!("add".parse(), Ok(ContentMarker::Add(())));
        assert_eq!("addpn".parse(), Ok(ContentMarker::Addpn(())));
        assert_eq!("th".parse(), Ok(ContentMarker::Th((1, 1))));
        assert_eq!("th5".parse(), Ok(ContentMarker::Th((5, 5))));
        assert_eq!("th5-7".parse(), Ok(ContentMarker::Th((5, 7))));
        assert_eq!(
            "ts-s".parse(),
            Ok(MilestoneMarker::Ts((MilestoneSide::Start, ())))
        );
        assert_eq!(
            "ts-e".parse(),
            Ok(MilestoneMarker::Ts((MilestoneSide::End, ())))
        );
        assert_eq!(
            "qt-s".parse(),
            Ok(MilestoneMarker::Qt((MilestoneSide::Start, 1)))
        );
        assert_eq!(
            "qt-e".parse(),
            Ok(MilestoneMarker::Qt((MilestoneSide::End, 1)))
        );
        assert_eq!(
            "qt5-s".parse(),
            Ok(MilestoneMarker::Qt((MilestoneSide::Start, 5)))
        );
        assert_eq!(
            "qt5-e".parse(),
            Ok(MilestoneMarker::Qt((MilestoneSide::End, 5)))
        );
    }

    #[test]
    fn test_failure() {
        use ContentMarker as CM;
        use MilestoneMarker as MM;

        assert_eq!(
            CM::from_str("pt"),
            Err(MarkerParseError::UnknownMarker("pt".to_string()))
        );
        assert_eq!(CM::from_str("toc5"), Err(MarkerParseError::OutOfRange(3)));
        assert_matches!(CM::from_str("toc0"), Err(MarkerParseError::InvalidNumber(err)) => {
            assert_eq!(*err.kind(), IntErrorKind::Zero);
        });
        assert_matches!(CM::from_str("th1-"), Err(MarkerParseError::InvalidNumber(err)) => {
            assert_eq!(*err.kind(), IntErrorKind::Empty);
        });
        assert_eq!(
            MM::from_str("qt3"),
            Err(MarkerParseError::UnknownMarker("qt3".to_string())),
        );
        assert_eq!(MM::from_str("qt6-s"), Err(MarkerParseError::OutOfRange(5)));
    }
}
