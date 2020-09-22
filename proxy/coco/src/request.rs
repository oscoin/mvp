//! An API for keeping track of requests and their state transitions.
//!
//! See [`Request`] and [`waiting_room::WaitingRoom`] for a high-level view of the API.

use std::{collections::HashMap, marker::PhantomData};

use either::Either;
use serde::{Deserialize, Serialize};

use librad::{net::peer::types::Gossip, peer::PeerId, uri::RadUrn};

/// The state types and traits that we can use for [`Request`]'s `S` parameter.
pub mod states;
pub use states::*;
/// The enumeration of different [`Request`] states unified under a single enum called
/// [`SomeRequest`].
pub mod existential;
pub use existential::SomeRequest;
/// The black box tracker of [`Request`]s and their lifecycles.
pub mod waiting_room;

mod sealed;

/// The maximum number of query attempts that can be made for a single request.
const MAX_QUERIES: Queries = Queries::new(1);

/// The maximum number of clone attempts that can be made for a single request.
const MAX_CLONES: Clones = Clones::new(1);

/// A `Request` represents the lifetime of requesting an identity in the network via its
/// [`RadUrn`].
///
/// The `Request`'s state is represented by the `S` type parameter. This parameter makes sure that
/// a `Request` transitions through specific states in a type safe manner.
///
/// These transitions are pictured below:
///
/// ```text
///      +----------------------------------v
///      |                             +---------+
///      |                   +-------->+cancelled+<------+
///      |                   |         +----+----+       |
///      |                   |              ^            |
///      |                   |              |            |
/// +----+----+       +------+--+       +---+-+      +---+---+       +------+
/// | created +------>+requested+------>+found+----->+cloning+------>+cloned|
/// +---------+       +------+--+       +--+--+      +---+---+       +------+
///                          |             |  ^------+   |
///                          |             |   failed    |
///                          |             v             |
///                          |          +--+------+      |
///                          +--------->+timed out+------+
///                                     +---------+
/// ```
///
/// The `T` type parameter represents some timestamp that is chosen by the user of the `Request`
/// API. Note that it makes it easy to test by just choosing `()` for the timestamp.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request<S, T> {
    /// The identifier of the identity on the network.
    urn: RadUrn,
    /// The number of attempts this request has made to complete its job.
    attempts: Attempts,
    /// The timestamp of the latest action to be taken on this request.
    timestamp: T,
    /// The state of the request, as mentioned above.
    state: S,
}

impl<S, T> From<Request<S, T>> for Gossip {
    fn from(request: Request<S, T>) -> Self {
        Self {
            urn: request.urn,
            rev: None,
            origin: None,
        }
    }
}

impl<S, T> Request<S, T> {
    /// Get the [`RadUrn`] that this `Request` is searching for.
    pub fn urn(&self) -> &RadUrn {
        &self.urn
    }

    /// Transition this `Request` into an `IsCanceled` state. We can only transition a particular
    /// subset of the states which are: `{IsCreated, IsRequested, Found, Cloning, IsCanceled}`.
    ///
    /// That is, attempting to cancel a `Cloned` `Request` is not permitted and will complain at
    /// compile time.
    pub fn cancel(self, timestamp: T) -> Request<IsCanceled, T>
    where
        S: Cancel,
    {
        Request {
            urn: self.urn,
            attempts: self.attempts,
            timestamp,
            state: self.state.cancel(),
        }
    }

    /// If a state keeps track of found peers then it can transition to back to itself by adding a
    /// `PeerId` to the existing set of peers.
    pub fn found(mut self, peer: PeerId, timestamp: T) -> Request<S, T>
    where
        S: HasPeers,
    {
        self.state.peers().entry(peer).or_insert(Status::Available);
        self.timestamp = timestamp;
        self
    }

    /// A `Request` transitions into a timed out state if it exceeds the maximum number of queries
    /// or maximum number of clones. Otherwise, the `Request` proceeds as normal.
    ///
    /// The subset of states that can transition to the `TimedOut` out state consist of
    /// `{IsRequested, Found, Cloning}`.
    pub fn timed_out(
        mut self,
        max_queries: Queries,
        max_clones: Clones,
        timestamp: T,
    ) -> Either<Self, Request<TimedOut, T>>
    where
        S: TimeOut,
    {
        if self.attempts.queries > max_queries {
            Either::Right(Request {
                urn: self.urn,
                attempts: self.attempts,
                timestamp,
                state: self.state.time_out(TimedOut::Query),
            })
        } else if self.attempts.clones > max_clones {
            Either::Right(Request {
                urn: self.urn,
                attempts: self.attempts,
                timestamp,
                state: self.state.time_out(TimedOut::Clone),
            })
        } else {
            self.timestamp = timestamp;
            Either::Left(self)
        }
    }

    /// When a `Request` is queried it we increment the `queries` count -- tracked via the
    /// `attempts` of the `Request`. If incrementing this count makes it exceed the maximum then
    /// the `Request` transitions into the `TimedOut` out state.
    pub fn queried(
        mut self,
        max_queries: Queries,
        max_clones: Clones,
        timestamp: T,
    ) -> Either<Request<TimedOut, T>, Self>
    where
        S: TimeOut + QueryAttempt,
    {
        self.attempts.queries += 1;
        self.timed_out(max_queries, max_clones, timestamp).flip()
    }
}

impl<T> Request<IsCreated, T> {
    /// Create a fresh `Request` for the give `urn`.
    ///
    /// Once this request has been made, we can transition this `Request` to the `IsRequested`
    /// state by calling [`Request::request`].
    pub fn new(urn: RadUrn, timestamp: T) -> Self {
        Self {
            urn,
            attempts: Attempts::new(),
            timestamp,
            state: PhantomData,
        }
    }

    /// Transition the `Request` from the `IsCreated` state to the `IsRequested` state.
    ///
    /// This signifies that the `Request` has been queried and will be looking for peers to fulfill
    /// the request.
    ///
    /// The number of queries is incremented by 1.
    pub fn request(self, timestamp: T) -> Request<IsRequested, T> {
        Request {
            urn: self.urn,
            attempts: Attempts {
                queries: self.attempts.queries + 1,
                ..self.attempts
            },
            timestamp,
            state: PhantomData,
        }
    }
}

impl<T> Request<IsRequested, T> {
    /// Transition the `Request` from the `IsRequested` state to the `Found` state.
    ///
    /// This signifies that the `Request` found its first peer and will be ready to attempt to
    /// clone from the peer.
    pub fn first_peer(self, peer: PeerId, timestamp: T) -> Request<Found, T> {
        let mut peers = HashMap::new();
        peers.insert(peer, Status::Available);
        Request {
            urn: self.urn,
            attempts: self.attempts,
            timestamp,
            state: Found { peers },
        }
    }
}

// TODO(finto): I think we need a state to transition back to `IsRequested` if there's no peers
// left to attempt cloning from.
impl<T> Request<Found, T> {
    /// Transition the `Request` from the `Found` state to the `Cloning` state.
    ///
    /// This signifies that the `Request` is attempting to clone from the provided `peer`.
    pub fn cloning(
        self,
        max_queries: Queries,
        max_clones: Clones,
        peer: PeerId,
        timestamp: T,
    ) -> Either<Request<TimedOut, T>, Request<Cloning, T>>
    where
        T: Clone,
    {
        let mut peers = self.state.peers;
        peers
            .entry(peer)
            .and_modify(|status| *status = Status::InProgress)
            .or_insert(Status::InProgress);
        let this = Request {
            urn: self.urn,
            attempts: Attempts {
                queries: self.attempts.queries,
                clones: self.attempts.clones + 1,
            },
            timestamp: timestamp.clone(),
            state: Cloning { peers },
        };
        this.timed_out(max_queries, max_clones, timestamp).flip()
    }
}

impl<T> Request<Cloning, T> {
    /// Transition from the `Cloning` state back to the `Found` state.
    ///
    /// This signifies that the `peer` failed to clone the identity and we mark it as failed.
    pub fn failed(self, peer: PeerId, timestamp: T) -> Request<Found, T> {
        let mut peers = self.state.peers;
        // TODO(finto): It's weird if it didn't exist but buh
        peers
            .entry(peer)
            .and_modify(|status| *status = Status::Failed)
            .or_insert(Status::Failed);
        Request {
            urn: self.urn,
            attempts: self.attempts,
            timestamp,
            state: Found { peers },
        }
    }

    /// Transition from the `Cloning` to the `Cloned` state.
    ///
    /// This signifies that the clone was successful and that the whole request was successful,
    /// congratulations.
    pub fn cloned(self, repo: RadUrn, timestamp: T) -> Request<Cloned, T> {
        Request {
            urn: self.urn,
            attempts: self.attempts,
            timestamp,
            state: Cloned { repo },
        }
    }
}

/// Due to the lack of higher-kinded types we have to write our own specific sequence here that
/// works with a `Result` embedded in an `Either`.
fn sequence_result<A, B, E>(either: Either<A, Result<B, E>>) -> Result<Either<A, B>, E> {
    match either {
        Either::Left(a) => Ok(Either::Left(a)),
        Either::Right(r) => Ok(Either::Right(r?)),
    }
}
