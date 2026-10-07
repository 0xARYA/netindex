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
        let width = if ipv6 { 128 } else { 32 };
        if !ipv6 && (self.start > u128::from(u32::MAX) || self.end > u128::from(u32::MAX)) {
            return Err(Error::Invalid("target bounds"));
        }

        let address = ip(self.start, ipv6);
        let target = match self.kind {
            0 => {
                if self.prefix != width || self.start != self.end {
                    return Err(Error::Invalid("target bounds"));
                }

                Target::Address(address)
            }
            1 => {
                if self.prefix > width {
                    return Err(Error::Invalid("network prefix"));
                }

                let host = host_mask(width - self.prefix);
                if self.start & host != 0 || self.end != self.start | host {
                    return Err(Error::Invalid("target bounds"));
                }

                Target::Network {
                    address,
                    prefix: self.prefix,
                }
            }
            2 => {
                if self.start > self.end || self.prefix != 0 {
                    return Err(Error::Invalid("target bounds"));
                }

                Target::Range {
                    start: address,
                    end: ip(self.end, ipv6),
                }
            }
            _ => return Err(Error::Invalid("target kind")),
        };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_targets_preserve_every_prefix_and_reject_invalid_bounds() {
        for ipv6 in [false, true] {
            let width = if ipv6 { 128 } else { 32 };
            for prefix in 0..=width {
                let end = host_mask(width - prefix);
                let entry = Entry {
                    start: 0,
                    end,
                    maximum: end,
                    id: 7,
                    prefix,
                    kind: 1,
                };
                let target = entry.target(ipv6).unwrap();
                let (roundtrip, family) = target.entry(entry.id).unwrap();

                assert_eq!(family, ipv6);
                assert_eq!((roundtrip.start, roundtrip.end), (entry.start, entry.end));
                assert_eq!(roundtrip.prefix, prefix);

                if prefix < width {
                    assert!(Entry { start: 1, ..entry }.target(ipv6).is_err());
                    assert!(Entry {
                        end: end - 1,
                        ..entry
                    }
                    .target(ipv6)
                    .is_err());
                }
            }
        }

        let entry = Entry {
            start: 1,
            end: 1,
            maximum: 1,
            id: 0,
            prefix: 32,
            kind: 0,
        };
        for invalid in [
            Entry {
                prefix: 31,
                ..entry
            },
            Entry { end: 2, ..entry },
            Entry {
                start: 1u128 << 32,
                end: 1u128 << 32,
                ..entry
            },
            Entry {
                kind: 1,
                prefix: 33,
                ..entry
            },
            Entry {
                kind: 2,
                prefix: 0,
                end: 0,
                ..entry
            },
            Entry {
                kind: 2,
                prefix: 1,
                ..entry
            },
            Entry { kind: 3, ..entry },
        ] {
            assert!(invalid.target(false).is_err());
        }
    }
}
