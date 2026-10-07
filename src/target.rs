use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::Error;

/// Original subject of one assertion; no ranges or ASNs are expanded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// One IPv4 or IPv6 address.
    Address(IpAddr),
    /// Canonical network address and prefix length.
    Network {
        /// Network address with host bits cleared.
        address: IpAddr,
        /// Prefix length in that address family.
        prefix: u8,
    },
    /// Inclusive endpoints in the same address family.
    Range {
        /// First included address.
        start: IpAddr,
        /// Last included address.
        end: IpAddr,
    },
    /// Nonzero exact ASN key.
    Asn(u32),
}

impl Target {
    pub(crate) fn entry(self, id: u32) -> Result<(Entry, bool), Error> {
        let (start, end, prefix, kind) = match self {
            Self::Address(address) => (address, address, bits(address), 0),
            Self::Network { address, prefix } => {
                let width = bits(address);
                if prefix > width {
                    return Err(Error::Invalid("network prefix"));
                }

                let value = number(address);
                let host = host_mask(width - prefix);
                if value & host != 0 {
                    return Err(Error::Invalid("network host bits"));
                }

                (address, ip(value | host, address.is_ipv6()), prefix, 1)
            }
            Self::Range { start, end } => {
                if start.is_ipv6() != end.is_ipv6() || number(start) > number(end) {
                    return Err(Error::Invalid("range endpoints"));
                }

                (start, end, 0, 2)
            }
            Self::Asn(_) => return Err(Error::Invalid("IP target")),
        };

        Ok((
            Entry {
                start: number(start),
                end: number(end),
                maximum: number(end),
                id,
                prefix,
                kind,
            },
            start.is_ipv6(),
        ))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Entry<N = u128> {
    pub start: N,
    pub end: N,
    pub maximum: N,
    pub id: u32,
    pub prefix: u8,
    pub kind: u8,
}

impl Entry {
    pub(crate) fn target(self, ipv6: bool) -> Result<Target, Error> {
        let address = ip(self.start, ipv6);
        let target = match self.kind {
            0 => Target::Address(address),
            1 => Target::Network {
                address,
                prefix: self.prefix,
            },
            2 => Target::Range {
                start: address,
                end: ip(self.end, ipv6),
            },
            _ => return Err(Error::Invalid("target kind")),
        };

        let (expected, _) = target.entry(self.id)?;
        if expected.end != self.end || expected.prefix != self.prefix {
            return Err(Error::Invalid("target bounds"));
        }

        Ok(target)
    }
}

impl<N: Copy> Entry<N> {
    pub(crate) fn map<M>(self, convert: impl Fn(N) -> M) -> Entry<M> {
        Entry {
            start: convert(self.start),
            end: convert(self.end),
            maximum: convert(self.maximum),
            id: self.id,
            prefix: self.prefix,
            kind: self.kind,
        }
    }
}

pub(crate) fn number(address: IpAddr) -> u128 {
    match address {
        IpAddr::V4(address) => u128::from(u32::from(address)),
        IpAddr::V6(address) => u128::from(address),
    }
}

fn bits(address: IpAddr) -> u8 {
    if address.is_ipv6() {
        128
    } else {
        32
    }
}

fn host_mask(bits: u8) -> u128 {
    if bits == 128 {
        u128::MAX
    } else {
        (1u128 << bits) - 1
    }
}

fn ip(value: u128, ipv6: bool) -> IpAddr {
    if ipv6 {
        Ipv6Addr::from(value).into()
    } else {
        Ipv4Addr::from(value as u32).into()
    }
}
