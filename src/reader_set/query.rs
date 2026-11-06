use std::{net::IpAddr, ops::ControlFlow};

use crate::{BatchError, Error, Match, Query, ReaderSet, Target};

impl<B: AsRef<[u8]>> ReaderSet<B> {
    /// Visit batches in query order, then source order, then each reader's order.
    ///
    /// Borrowed results and exact-prefix failures follow [`crate::Reader::visit_batch`].
    /// No result buffers or reordering; callback positions are query, then source.
    ///
    /// # Errors
    /// Returns the failed query position and preserves the original error.
    pub fn visit_batch<'a, E: From<Error>>(
        &'a self,
        queries: &[Query],
        mut emit: impl FnMut(usize, usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), BatchError<E>> {
        for (query_index, &query) in queries.iter().enumerate() {
            let result = match query {
                Query::Ip(ip) => self.visit_ip(ip, |source, row| emit(query_index, source, row)),
                Query::Asn(key) => {
                    self.visit_asn(key, |source, row| emit(query_index, source, row))
                }
                Query::Intersecting(target) => {
                    self.visit_intersecting(target, |source, row| emit(query_index, source, row))
                }
            };
            result.map_err(|error| BatchError { query_index, error })?;
        }
        Ok(())
    }

    /// Visit IP matches until a successful `Break(value)`, preserving source order.
    ///
    /// # Errors
    /// Preserves reader/visitor failures separately from successful termination.
    pub fn try_visit_ip<'a, T, E: From<Error>>(
        &'a self,
        address: IpAddr,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<ControlFlow<T>, E>,
    ) -> Result<ControlFlow<T>, E> {
        for source in crate::coverage::candidates(self.router.mask(address)) {
            let reader = self
                .readers
                .get(source)
                .ok_or(Error::Invalid("routed reader position"))?;
            if let ControlFlow::Break(value) =
                reader.try_visit_ip(address, |row| emit(source, row))?
            {
                return Ok(ControlFlow::Break(value));
            }
        }
        Ok(ControlFlow::Continue(()))
    }

    /// Visit exact-ASN matches with successful early stopping and source ordering.
    ///
    /// # Errors
    /// Preserves reader and visitor errors; zero is a miss.
    pub fn try_visit_asn<'a, T, E: From<Error>>(
        &'a self,
        number: u32,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<ControlFlow<T>, E>,
    ) -> Result<ControlFlow<T>, E> {
        for (source, reader) in self.readers.iter().enumerate() {
            if let ControlFlow::Break(value) =
                reader.try_visit_asn(number, |row| emit(source, row))?
            {
                return Ok(ControlFlow::Break(value));
            }
        }
        Ok(ControlFlow::Continue(()))
    }

    /// Test IP membership, stopping at the first matching source/assertion.
    ///
    /// # Errors
    /// Returns a reader error encountered before the first match.
    pub fn contains_ip(&self, address: IpAddr) -> Result<bool, Error> {
        Ok(self
            .try_visit_ip(address, |_, _| Ok::<_, Error>(ControlFlow::Break(())))?
            .is_break())
    }

    /// Test exact-ASN membership, stopping at the first match; zero is a miss.
    ///
    /// # Errors
    /// Returns a reader error encountered before the first match.
    pub fn contains_asn(&self, number: u32) -> Result<bool, Error> {
        Ok(self
            .try_visit_asn(number, |_, _| Ok::<_, Error>(ControlFlow::Break(())))?
            .is_break())
    }

    /// Visit inclusive intersections in source order, then each reader's order.
    ///
    /// Each reader follows [`crate::Reader::visit_intersecting`]. All sources are
    /// searched; point-query coverage masks cannot safely prune interval queries.
    ///
    /// # Errors
    /// Validates the target even for empty sets, then preserves reader/visitor errors.
    pub fn visit_intersecting<'a, E: From<Error>>(
        &'a self,
        target: Target,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<(), E>,
    ) -> Result<(), E> {
        target.entry(0)?;
        for (source, reader) in self.readers.iter().enumerate() {
            reader.visit_intersecting(target, |row| emit(source, row))?;
        }
        Ok(())
    }

    /// Visit intersections with successful early stopping, in source order.
    ///
    /// # Errors
    /// Validates the target before delivery, preserving reader/visitor errors.
    pub fn try_visit_intersecting<'a, T, E: From<Error>>(
        &'a self,
        target: Target,
        mut emit: impl FnMut(usize, Match<'a>) -> Result<ControlFlow<T>, E>,
    ) -> Result<ControlFlow<T>, E> {
        target.entry(0)?;
        for (source, reader) in self.readers.iter().enumerate() {
            if let ControlFlow::Break(value) =
                reader.try_visit_intersecting(target, |row| emit(source, row))?
            {
                return Ok(ControlFlow::Break(value));
            }
        }
        Ok(ControlFlow::Continue(()))
    }

    /// Stop a batch successfully, returning the input query position and value.
    ///
    /// # Errors
    /// Uses [`Self::visit_batch`]'s exact-prefix error contract; later queries do not run.
    pub fn try_visit_batch<'a, T, E: From<Error>>(
        &'a self,
        queries: &[Query],
        mut emit: impl FnMut(usize, usize, Match<'a>) -> Result<ControlFlow<T>, E>,
    ) -> Result<ControlFlow<(usize, T)>, BatchError<E>> {
        for (query_index, &query) in queries.iter().enumerate() {
            let result = match query {
                Query::Ip(ip) => {
                    self.try_visit_ip(ip, |source, row| emit(query_index, source, row))
                }
                Query::Asn(key) => {
                    self.try_visit_asn(key, |source, row| emit(query_index, source, row))
                }
                Query::Intersecting(target) => self
                    .try_visit_intersecting(target, |source, row| emit(query_index, source, row)),
            }
            .map_err(|error| BatchError { query_index, error })?;
            if let ControlFlow::Break(value) = result {
                return Ok(ControlFlow::Break((query_index, value)));
            }
        }
        Ok(ControlFlow::Continue(()))
    }
}
