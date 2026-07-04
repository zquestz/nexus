//! Weighted fair queuing core for server egress scheduling.
//!
//! This module is deliberately isolated from sockets and async I/O. It owns
//! virtual-time accounting, flow membership, intra-flow priority, and blocked
//! connection skipping; the egress task wraps this core with rate-budget and
//! writer-channel integration.

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, VecDeque};

use nexus_common::validators::{DEFAULT_ADMIN_BANDWIDTH_WEIGHT, MIN_BANDWIDTH_WEIGHT};

use crate::constants::ERR_SCHEDULER_PEEKED_ENTRY_MISSING;

const VIRTUAL_TIME_SCALE: u128 = 1_000_000;

/// A start-ordered heap is compacted (superseded entries dropped) once its
/// length exceeds `dispatchable_count * HEAP_COMPACT_FACTOR + HEAP_COMPACT_SLACK`.
/// Because that only triggers after the heap has grown several times past its
/// live size, compaction is amortized O(1) per push and keeps the heaps bounded
/// even under refresh churn that never dispatches (e.g. repeated weight changes).
const HEAP_COMPACT_FACTOR: usize = 4;
const HEAP_COMPACT_SLACK: usize = 16;

/// Pre-login traffic shares one global anonymous flow.
pub const ANON_FLOW_WEIGHT: u16 = DEFAULT_ADMIN_BANDWIDTH_WEIGHT;

/// User-flow identity used by WF2Q+.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FlowId {
    Anon,
    User(i64),
}

/// Per-connection identity owned by the scheduler integration layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnectionId(u64);

impl ConnectionId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A connection's fixed scheduling class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionClass {
    Protocol,
    Bulk,
}

/// A packet's priority within one scheduler connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacketPriority {
    Normal,
    Priority,
}

/// Packet stored in the scheduler queue.
#[derive(Debug, PartialEq, Eq)]
pub struct SchedulerPacket<T> {
    pub bytes: usize,
    pub payload: T,
    pub priority: PacketPriority,
    /// Whether this packet completes a logical frame. The scheduler will not
    /// serve another queue's packets until a connection's in-progress frame
    /// reaches its final chunk, so a priority frame cannot interleave inside a
    /// partially dispatched normal frame. A standalone packet is a complete
    /// frame (`true`); a non-final chunk of a multi-chunk frame is `false`.
    pub is_final: bool,
}

impl<T> SchedulerPacket<T> {
    pub fn new(bytes: usize, payload: T) -> Self {
        Self {
            bytes,
            payload,
            priority: PacketPriority::Normal,
            is_final: true,
        }
    }

    pub fn priority(bytes: usize, payload: T) -> Self {
        Self {
            bytes,
            payload,
            priority: PacketPriority::Priority,
            is_final: true,
        }
    }
}

/// Packet returned by a scheduler dequeue.
#[derive(Debug, PartialEq, Eq)]
pub struct DequeuedPacket<T> {
    pub connection_id: ConnectionId,
    pub flow_id: FlowId,
    pub class: ConnectionClass,
    pub bytes: usize,
    pub payload: T,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EnqueueError<T> {
    UnknownConnection {
        connection_id: ConnectionId,
        packet: SchedulerPacket<T>,
    },
    EmptyPacket {
        connection_id: ConnectionId,
        packet: SchedulerPacket<T>,
    },
}

/// Isolated WF2Q+ scheduler core.
pub struct Wf2qScheduler<T> {
    flows: HashMap<FlowId, FlowState>,
    connections: HashMap<ConnectionId, ConnectionState<T>>,
    virtual_time: u128,
    total_flow_weight_sum: u128,
    /// Number of flows with a currently dispatchable connection. Maintained
    /// incrementally via `refresh_flow_candidate` at every mutation site so
    /// `has_dispatchable_packet` is O(1) instead of an O(flows) scan. A debug
    /// invariant recomputes it by scan after every mutation to prove the wiring.
    dispatchable_count: usize,
    /// Min-heap (by start) of every flow's current dispatch candidate, so
    /// `min_dispatchable_start` is O(log flows) instead of an O(flows) scan.
    /// Never drained except by lazy stale-skip — it must keep eligible entries
    /// too, since the min-start floor is taken over all candidates.
    start_heap: BinaryHeap<Reverse<ByStart>>,
    /// Min-heap (by start) of candidates not yet known to be eligible. Drained
    /// into `eligible_heap` as virtual time crosses each entry's start. Distinct
    /// from `start_heap` precisely because this one *is* drained.
    pending_heap: BinaryHeap<Reverse<ByStart>>,
    /// Min-heap (by finish) of candidates whose start ≤ virtual_time. The WF2Q
    /// pick among eligible candidates is this heap's current top.
    eligible_heap: BinaryHeap<Reverse<ByFinish>>,
}

/// A per-flow dispatch candidate carried in the scheduler's heaps. Holds
/// everything `select_candidate` needs to build a `Candidate`, plus the flow
/// `generation` at push time — an entry is current only while it matches
/// `FlowState::generation`, so superseded entries are discarded lazily.
#[derive(Clone, Copy)]
struct CandidateEntry {
    start: u128,
    finish: u128,
    flow_id: FlowId,
    connection_id: ConnectionId,
    class: ConnectionClass,
    member_index: usize,
    generation: u64,
}

/// `CandidateEntry` ordered by virtual start (then finish / flow / connection
/// for determinism). `Reverse<ByStart>` in a `BinaryHeap` yields a min-heap on
/// start. `ConnectionClass` isn't `Ord`, so ordering is by explicit key.
#[derive(Clone, Copy)]
struct ByStart(CandidateEntry);

impl ByStart {
    fn key(&self) -> (u128, u128, FlowId, ConnectionId) {
        (
            self.0.start,
            self.0.finish,
            self.0.flow_id,
            self.0.connection_id,
        )
    }
}

impl PartialEq for ByStart {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}
impl Eq for ByStart {}
impl PartialOrd for ByStart {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ByStart {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

/// `CandidateEntry` ordered by virtual finish (then flow / connection) — the
/// WF2Q tie-break for picking among eligible candidates. Mirrors
/// `keep_best_candidate`'s `(finish, flow_id, connection_id)` ordering.
#[derive(Clone, Copy)]
struct ByFinish(CandidateEntry);

impl ByFinish {
    fn key(&self) -> (u128, FlowId, ConnectionId) {
        (self.0.finish, self.0.flow_id, self.0.connection_id)
    }
}

impl PartialEq for ByFinish {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}
impl Eq for ByFinish {}
impl PartialOrd for ByFinish {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ByFinish {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

impl<T> Default for Wf2qScheduler<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Wf2qScheduler<T> {
    pub fn new() -> Self {
        Self {
            flows: HashMap::new(),
            connections: HashMap::new(),
            virtual_time: 0,
            total_flow_weight_sum: 0,
            dispatchable_count: 0,
            start_heap: BinaryHeap::new(),
            pending_heap: BinaryHeap::new(),
            eligible_heap: BinaryHeap::new(),
        }
    }

    /// Recompute `flow_id`'s current dispatch candidate after any mutation that
    /// can change it (enqueue, dequeue, ack, block/unblock, membership,
    /// transition): adjust the maintained `dispatchable_count` and push a fresh
    /// `start_heap` entry (bumping the flow's generation so prior entries go
    /// stale). No-op for an absent flow.
    ///
    /// The candidate's start is read from the connection's cached `head_tags`
    /// when present (set by `tag_initial`/`tag_next`), else computed as
    /// `max(last_finish, virtual_time)` — exactly what `ensure_candidate_tags`
    /// would lazily compute at select time. `virtual_time` never advances
    /// between a flow becoming dispatchable and its next `min_dispatchable_start`
    /// query (every advance goes through that query), so the snapshot matches
    /// the lazy value; the debug cross-check enforces this.
    fn refresh_flow_candidate(&mut self, flow_id: FlowId) {
        let candidate = self.flows.get(&flow_id).and_then(|flow| {
            Self::dispatchable_connection_for_flow(&self.connections, flow)
                .map(|selection| (selection, flow.last_finish, flow.weight))
        });
        let now_dispatchable = candidate.is_some();

        let virtual_time = self.virtual_time;
        let entry = candidate.and_then(|(selection, last_finish, weight)| {
            let connection = self.connections.get_mut(&selection.connection_id)?;
            // Eager-cache the candidate's tags so its start is fixed once the
            // connection reaches the head of line (textbook WF2Q+), rather than
            // drifting with virtual_time on later refreshes. This replaces the
            // caching the old linear select used to perform.
            let tags = if let Some(tags) = connection.head_tags {
                tags
            } else {
                let bytes = connection.front_packet()?.bytes;
                let start = last_finish.max(virtual_time);
                let tags = PacketTags {
                    start,
                    finish: start + scaled_work(bytes, u128::from(weight)),
                };
                connection.head_tags = Some(tags);
                tags
            };
            Some((selection, tags.start, tags.finish))
        });

        let Some(flow) = self.flows.get_mut(&flow_id) else {
            return;
        };
        if flow.dispatchable != now_dispatchable {
            flow.dispatchable = now_dispatchable;
            if now_dispatchable {
                self.dispatchable_count += 1;
            } else {
                self.dispatchable_count -= 1;
            }
        }
        flow.generation += 1;
        let generation = flow.generation;

        if let Some((selection, start, finish)) = entry {
            let candidate = CandidateEntry {
                start,
                finish,
                flow_id,
                connection_id: selection.connection_id,
                class: selection.class,
                member_index: selection.member_index,
                generation,
            };
            // start_heap keeps every candidate (for the min-start floor);
            // pending_heap is drained into eligible_heap as virtual time rises.
            self.start_heap.push(Reverse(ByStart(candidate)));
            self.pending_heap.push(Reverse(ByStart(candidate)));
        }

        // Bound the start-ordered heaps against refresh churn: each refresh
        // orphans the flow's prior entries, and lazy stale-skip only evicts at
        // the top, so without this they grow without dispatch ever draining them.
        let live = self.dispatchable_count;
        if self.start_heap.len() > live * HEAP_COMPACT_FACTOR + HEAP_COMPACT_SLACK {
            Self::compact_start_heap(&mut self.start_heap, &self.flows);
        }
        if self.pending_heap.len() > live * HEAP_COMPACT_FACTOR + HEAP_COMPACT_SLACK {
            Self::compact_start_heap(&mut self.pending_heap, &self.flows);
        }
    }

    /// Rebuild a start-ordered heap, dropping superseded entries (generation no
    /// longer matching the flow). Removing stale entries is purely an
    /// optimization — they are never returned — so this is behavior-preserving.
    fn compact_start_heap(
        heap: &mut BinaryHeap<Reverse<ByStart>>,
        flows: &HashMap<FlowId, FlowState>,
    ) {
        *heap = std::mem::take(heap)
            .into_iter()
            .filter(|Reverse(entry)| {
                flows
                    .get(&entry.0.flow_id)
                    .is_some_and(|flow| flow.generation == entry.0.generation)
            })
            .collect();
    }

    /// Drop a removed flow's contribution to `dispatchable_count`. Call before
    /// removing a flow from the map (while its `dispatchable` flag is still
    /// readable).
    fn release_flow_dispatchable(&mut self, flow_id: FlowId) {
        if self
            .flows
            .get(&flow_id)
            .is_some_and(|flow| flow.dispatchable)
        {
            self.dispatchable_count -= 1;
        }
    }

    /// Debug-only: recompute `dispatchable_count` by full scan and assert it
    /// matches the incrementally-maintained value. The cross-check that proves
    /// no mutation site was missed.
    #[cfg(debug_assertions)]
    fn debug_assert_dispatchable_count(&self) {
        let scanned = self
            .flows
            .values()
            .filter(|flow| {
                Self::dispatchable_connection_for_flow(&self.connections, flow).is_some()
            })
            .count();
        debug_assert_eq!(
            self.dispatchable_count, scanned,
            "dispatchable_count drifted from scan — a mutation site is missing refresh_flow_candidate"
        );
    }

    #[cfg(not(debug_assertions))]
    #[inline]
    fn debug_assert_dispatchable_count(&self) {}

    pub fn register_connection(
        &mut self,
        connection_id: ConnectionId,
        flow_id: FlowId,
        class: ConnectionClass,
        weight: u16,
    ) -> bool {
        if self.connections.contains_key(&connection_id) {
            return false;
        }

        let weight = normalize_weight(weight);
        let should_clear_tags = if let Some(flow) = self.flows.get_mut(&flow_id) {
            let weight_changed = flow.weight != weight;
            if weight_changed {
                self.total_flow_weight_sum = self
                    .total_flow_weight_sum
                    .saturating_sub(u128::from(flow.weight))
                    + u128::from(weight);
                flow.weight = weight;
                flow.last_finish = flow.last_finish.max(self.virtual_time);
            }
            flow.add_connection(class, connection_id);
            weight_changed
        } else {
            self.flows
                .insert(flow_id, FlowState::new(weight, self.virtual_time));
            self.total_flow_weight_sum += u128::from(weight);
            if let Some(flow) = self.flows.get_mut(&flow_id) {
                flow.add_connection(class, connection_id);
            }
            false
        };

        self.connections.insert(
            connection_id,
            ConnectionState {
                flow_id,
                class,
                blocked: false,
                in_flight: false,
                head_tags: None,
                priority_queue: VecDeque::new(),
                queue: VecDeque::new(),
                active_queue: None,
            },
        );

        if should_clear_tags {
            self.clear_flow_tags(flow_id);
        }

        self.refresh_flow_candidate(flow_id);
        self.debug_assert_dispatchable_count();
        true
    }

    pub fn register_anon_connection(
        &mut self,
        connection_id: ConnectionId,
        class: ConnectionClass,
    ) -> bool {
        self.register_connection(connection_id, FlowId::Anon, class, ANON_FLOW_WEIGHT)
    }

    pub fn register_user_connection(
        &mut self,
        connection_id: ConnectionId,
        user_id: i64,
        class: ConnectionClass,
        weight: u16,
    ) -> bool {
        self.register_connection(connection_id, FlowId::User(user_id), class, weight)
    }

    pub fn unregister_connection(&mut self, connection_id: ConnectionId) -> bool {
        let Some(connection) = self.connections.remove(&connection_id) else {
            return false;
        };

        let clear_remaining_tags = !connection.is_empty() || connection.head_tags.is_some();
        self.remove_flow_member(
            connection.flow_id,
            connection.class,
            connection_id,
            clear_remaining_tags,
        );
        true
    }

    pub fn transition_to_user(
        &mut self,
        connection_id: ConnectionId,
        user_id: i64,
        weight: u16,
    ) -> bool {
        let Some(connection) = self.connections.get(&connection_id) else {
            return false;
        };

        let new_flow_id = FlowId::User(user_id);
        let weight = normalize_weight(weight);
        let old_flow_id = connection.flow_id;
        let class = connection.class;
        let connection_had_dispatch_state =
            !connection.is_empty() || connection.head_tags.is_some();
        if old_flow_id == new_flow_id {
            let Some(weight_changed) = self.update_existing_flow_weight(new_flow_id, weight) else {
                return false;
            };
            if weight_changed {
                self.clear_flow_tags(new_flow_id);
            }
            self.refresh_flow_candidate(new_flow_id);
            self.debug_assert_dispatchable_count();
            return true;
        }

        let Some(connection) = self.connections.get_mut(&connection_id) else {
            return false;
        };
        connection.flow_id = new_flow_id;
        connection.clear_tags();

        self.remove_flow_member(
            old_flow_id,
            class,
            connection_id,
            connection_had_dispatch_state,
        );

        let mut new_flow_weight_changed = false;
        {
            if self.flows.contains_key(&new_flow_id) {
                new_flow_weight_changed =
                    self.update_existing_flow_weight(new_flow_id, weight) == Some(true);
            } else {
                self.flows
                    .insert(new_flow_id, FlowState::new(weight, self.virtual_time));
                self.total_flow_weight_sum += u128::from(weight);
            }
            if let Some(flow) = self.flows.get_mut(&new_flow_id) {
                flow.add_connection(class, connection_id);
            }
        }
        if connection_had_dispatch_state || new_flow_weight_changed {
            self.clear_flow_tags(new_flow_id);
        }

        // The old flow was refreshed inside remove_flow_member; refresh the new.
        self.refresh_flow_candidate(new_flow_id);
        self.debug_assert_dispatchable_count();
        true
    }

    pub fn update_user_weight(&mut self, user_id: i64, weight: u16) -> bool {
        let flow_id = FlowId::User(user_id);
        let Some(weight_changed) =
            self.update_existing_flow_weight(flow_id, normalize_weight(weight))
        else {
            return false;
        };
        if weight_changed {
            self.clear_flow_tags(flow_id);
        }
        // A weight change bumps last_finish and clears tags, so the candidate's
        // start changes even though dispatchability doesn't — refresh the heap.
        self.refresh_flow_candidate(flow_id);
        self.debug_assert_dispatchable_count();
        true
    }

    pub fn set_connection_blocked(&mut self, connection_id: ConnectionId, blocked: bool) -> bool {
        let Some(connection) = self.connections.get_mut(&connection_id) else {
            return false;
        };

        let flow_id = connection.flow_id;
        if connection.blocked != blocked {
            connection.blocked = blocked;
            connection.clear_tags();
        }
        self.refresh_flow_candidate(flow_id);
        self.debug_assert_dispatchable_count();
        true
    }

    pub fn set_connection_in_flight(
        &mut self,
        connection_id: ConnectionId,
        in_flight: bool,
    ) -> bool {
        let Some(connection) = self.connections.get_mut(&connection_id) else {
            return false;
        };

        connection.in_flight = in_flight;
        let flow_id = connection.flow_id;
        self.refresh_flow_candidate(flow_id);
        self.debug_assert_dispatchable_count();
        true
    }

    pub fn enqueue(
        &mut self,
        connection_id: ConnectionId,
        packet: SchedulerPacket<T>,
    ) -> Result<(), EnqueueError<T>> {
        if packet.bytes == 0 {
            return Err(EnqueueError::EmptyPacket {
                connection_id,
                packet,
            });
        }

        let Some(connection) = self.connections.get(&connection_id) else {
            return Err(EnqueueError::UnknownConnection {
                connection_id,
                packet,
            });
        };
        let flow_id = connection.flow_id;
        let flow_weight = self
            .flows
            .get(&flow_id)
            .map_or(MIN_BANDWIDTH_WEIGHT, |flow| flow.weight);
        let was_dispatchable = self.flow_has_dispatchable_connection(flow_id);

        let Some(connection) = self.connections.get_mut(&connection_id) else {
            return Err(EnqueueError::UnknownConnection {
                connection_id,
                packet,
            });
        };
        let bytes = packet.bytes;
        let preempted_start = connection.push_packet(
            QueuedPacket {
                bytes,
                payload: packet.payload,
                is_final: packet.is_final,
            },
            packet.priority,
        );
        if let Some(start) = preempted_start {
            connection.head_tags = Some(PacketTags {
                start,
                finish: start + scaled_work(bytes, u128::from(flow_weight)),
            });
        }

        if !was_dispatchable {
            self.tag_initial_dispatchable_for_flow(flow_id);
        }

        // Push the refreshed candidate before the floor query, so the heap and
        // the linear scan see the same set.
        self.refresh_flow_candidate(flow_id);
        self.debug_assert_dispatchable_count();

        if !was_dispatchable {
            self.update_virtual_time_floor();
        }

        Ok(())
    }

    /// Dequeues the next packet and marks its connection in flight.
    ///
    /// The writer integration must clear the in-flight flag after the packet is
    /// written or unregister the connection on write failure.
    /// Callers must not drop a returned packet without taking one of those paths.
    pub fn dequeue(&mut self) -> Option<DequeuedPacket<T>> {
        let candidate = self.select_candidate()?;
        self.dequeue_candidate(candidate)
    }

    pub fn queued_packets(&self, connection_id: ConnectionId) -> Option<usize> {
        self.connections
            .get(&connection_id)
            .map(ConnectionState::queued_packets)
    }

    pub fn has_dispatchable_packet(&self) -> bool {
        self.debug_assert_dispatchable_count();
        self.dispatchable_count > 0
    }

    pub fn virtual_time(&self) -> u128 {
        self.virtual_time
    }

    /// WF2Q pick: the min-finish eligible candidate, or — if none is eligible —
    /// advance virtual time to the earliest start and pick that one. Heap-backed
    /// and O(log flows); the linear scan rides alongside in debug as the oracle.
    fn select_candidate(&mut self) -> Option<SelectedCandidate> {
        #[cfg(debug_assertions)]
        let expected = self.select_candidate_linear_key();

        let selected = self.select_candidate_heap();

        #[cfg(debug_assertions)]
        debug_assert_eq!(
            selected.map(|s| s.candidate.connection_id),
            expected,
            "select_candidate heap pick diverged from the linear scan"
        );

        selected
    }

    /// Heap-backed selection. Migrates newly-eligible candidates, then takes the
    /// min-finish eligible one; on an idle gap (nothing eligible) advances
    /// virtual time to the earliest start, migrates, and takes that one. Caches
    /// the winner's tags on its connection so `dequeue_candidate` can read them.
    fn select_candidate_heap(&mut self) -> Option<SelectedCandidate> {
        self.migrate_eligible(self.virtual_time);

        if let Some(entry) = self.peek_current_eligible() {
            return Some(self.finalize_selection(entry));
        }

        // Idle gap: jump virtual time to the earliest pending start, which makes
        // those candidates eligible, then pick among them.
        let min_start = self.peek_current_pending_start()?;
        self.virtual_time = min_start;
        self.migrate_eligible(min_start);
        let entry = self.peek_current_eligible()?;
        Some(self.finalize_selection(entry))
    }

    /// Move every current pending candidate with `start <= threshold` into the
    /// eligible heap. Stale entries are dropped; entries still above the
    /// threshold stay put (the heap is ordered by start, so we can stop early).
    fn migrate_eligible(&mut self, threshold: u128) {
        while let Some(Reverse(top)) = self.pending_heap.peek() {
            if top.0.start > threshold {
                break;
            }
            let Reverse(entry) = self
                .pending_heap
                .pop()
                .expect(ERR_SCHEDULER_PEEKED_ENTRY_MISSING);
            if self.entry_is_current(&entry.0) {
                self.eligible_heap.push(Reverse(ByFinish(entry.0)));
            }
        }
    }

    /// The min-finish eligible candidate, dropping stale entries off the top.
    /// Peeks (does not pop): the winner is invalidated by the post-dequeue
    /// refresh (new generation), so it self-evicts on the next migration.
    fn peek_current_eligible(&mut self) -> Option<CandidateEntry> {
        while let Some(Reverse(top)) = self.eligible_heap.peek() {
            if self.entry_is_current(&top.0) {
                return Some(top.0);
            }
            self.eligible_heap.pop();
        }
        None
    }

    /// The earliest current pending start, dropping stale entries off the top.
    fn peek_current_pending_start(&mut self) -> Option<u128> {
        while let Some(Reverse(top)) = self.pending_heap.peek() {
            if self.entry_is_current(&top.0) {
                return Some(top.0.start);
            }
            self.pending_heap.pop();
        }
        None
    }

    /// Cache the selected candidate's tags on its connection (so the immediate
    /// `dequeue_candidate` reads them) and build the `SelectedCandidate`.
    fn finalize_selection(&mut self, entry: CandidateEntry) -> SelectedCandidate {
        if let Some(connection) = self.connections.get_mut(&entry.connection_id) {
            connection.head_tags = Some(PacketTags {
                start: entry.start,
                finish: entry.finish,
            });
        }
        SelectedCandidate {
            candidate: Candidate {
                connection_id: entry.connection_id,
                class: entry.class,
                member_index: entry.member_index,
            },
        }
    }

    /// Read-only linear WF2Q pick used solely as the debug cross-check oracle.
    /// Computes candidate tags locally without caching them or advancing virtual
    /// time, so it can run beside the heap path without perturbing its state.
    /// Returns the winning connection id — which alone determines the dispatch,
    /// so it's sufficient to validate the heap pick (including its tie-break).
    #[cfg(debug_assertions)]
    fn select_candidate_linear_key(&self) -> Option<ConnectionId> {
        // WF2Q tie-break key: (finish, flow_id, connection_id), smaller wins.
        let mut min_start: Option<u128> = None;
        let mut best_eligible: Option<(u128, FlowId, ConnectionId)> = None;
        let mut best_after_idle: Option<(u128, FlowId, ConnectionId)> = None;

        for (flow_id, flow) in &self.flows {
            let Some(selection) = Self::dispatchable_connection_for_flow(&self.connections, flow)
            else {
                continue;
            };
            let Some(connection) = self.connections.get(&selection.connection_id) else {
                continue;
            };
            let (start, finish) = if let Some(tags) = connection.head_tags {
                (tags.start, tags.finish)
            } else {
                let Some(bytes) = connection.front_packet().map(|packet| packet.bytes) else {
                    continue;
                };
                let start = flow.last_finish.max(self.virtual_time);
                (start, start + scaled_work(bytes, u128::from(flow.weight)))
            };
            let key = (finish, *flow_id, selection.connection_id);

            if start <= self.virtual_time && best_eligible.is_none_or(|best| key < best) {
                best_eligible = Some(key);
            }
            match min_start {
                Some(current_min) if start > current_min => {}
                Some(current_min) if start == current_min => {
                    if best_after_idle.is_none_or(|best| key < best) {
                        best_after_idle = Some(key);
                    }
                }
                _ => {
                    min_start = Some(start);
                    best_after_idle = Some(key);
                }
            }
        }

        best_eligible
            .or(best_after_idle)
            .map(|(_, _, connection_id)| connection_id)
    }

    fn dispatchable_connection_for_flow(
        connections: &HashMap<ConnectionId, ConnectionState<T>>,
        flow: &FlowState,
    ) -> Option<FlowSelection> {
        Self::dispatchable_connection_in_class(
            connections,
            ConnectionClass::Protocol,
            &flow.protocol_connections,
            flow.protocol_cursor,
        )
        .or_else(|| {
            Self::dispatchable_connection_in_class(
                connections,
                ConnectionClass::Bulk,
                &flow.bulk_connections,
                flow.bulk_cursor,
            )
        })
    }

    fn dispatchable_connection_in_class(
        connection_states: &HashMap<ConnectionId, ConnectionState<T>>,
        class: ConnectionClass,
        members: &[ConnectionId],
        cursor: usize,
    ) -> Option<FlowSelection> {
        if members.is_empty() {
            return None;
        }

        (0..members.len()).find_map(|offset| {
            let idx = (cursor + offset) % members.len();
            let connection_id = members[idx];
            connection_states
                .get(&connection_id)
                .filter(|connection| {
                    // `front_packet()` (not `is_empty()`) so a connection locked
                    // to a momentarily-empty queue is not treated as dispatchable
                    // — it must wait for its in-progress frame to continue.
                    !connection.blocked
                        && !connection.in_flight
                        && connection.front_packet().is_some()
                })
                .map(|_| FlowSelection {
                    connection_id,
                    class,
                    member_index: idx,
                })
        })
    }

    fn dequeue_candidate(&mut self, selected: SelectedCandidate) -> Option<DequeuedPacket<T>> {
        let candidate = selected.candidate;
        let total_weight_sum = self.total_flow_weight_sum();

        let connection = self.connections.get_mut(&candidate.connection_id)?;
        let tags = connection.head_tags.take()?;
        let packet = connection.pop_front_packet()?;
        let bytes = packet.bytes;
        let flow_id = connection.flow_id;
        let class = connection.class;

        if let Some(flow) = self.flows.get_mut(&flow_id) {
            flow.last_finish = tags.finish;
            flow.advance_cursor(
                candidate.class,
                candidate.member_index,
                candidate.connection_id,
            );
        }
        self.tag_next_dispatchable_for_flow_after_dequeue(flow_id);
        if let Some(connection) = self.connections.get_mut(&candidate.connection_id) {
            connection.in_flight = true;
        }

        // Push the flow's refreshed candidate before querying min start, so the
        // heap and the linear scan see the same set.
        self.refresh_flow_candidate(flow_id);
        self.debug_assert_dispatchable_count();

        let advanced = self.virtual_time.max(tags.start) + scaled_work(bytes, total_weight_sum);
        self.virtual_time = self
            .min_dispatchable_start()
            .map_or(advanced, |min_start| advanced.max(min_start));

        Some(DequeuedPacket {
            connection_id: candidate.connection_id,
            flow_id,
            class,
            bytes,
            payload: packet.payload,
        })
    }

    fn tag_next_dispatchable_for_flow_after_dequeue(&mut self, flow_id: FlowId) {
        let Some((selection, start, weight)) = self.flows.get(&flow_id).and_then(|flow| {
            Self::dispatchable_connection_for_flow(&self.connections, flow)
                .map(|selection| (selection, flow.last_finish, flow.weight))
        }) else {
            return;
        };

        // Dispatch-affecting membership and blocked-state changes clear flow
        // tags, so this pre-tag cannot survive if another connection becomes
        // the dispatchable head first.
        let Some(connection) = self.connections.get_mut(&selection.connection_id) else {
            return;
        };
        let Some(bytes) = connection.front_packet().map(|packet| packet.bytes) else {
            return;
        };

        let finish = start + scaled_work(bytes, u128::from(weight));
        connection.head_tags = Some(PacketTags { start, finish });
    }

    fn tag_initial_dispatchable_for_flow(&mut self, flow_id: FlowId) {
        let Some((selection, start, weight)) = self.flows.get(&flow_id).and_then(|flow| {
            Self::dispatchable_connection_for_flow(&self.connections, flow).map(|selection| {
                (
                    selection,
                    flow.last_finish.max(self.virtual_time),
                    flow.weight,
                )
            })
        }) else {
            return;
        };

        let Some(connection) = self.connections.get_mut(&selection.connection_id) else {
            return;
        };
        let Some(bytes) = connection.front_packet().map(|packet| packet.bytes) else {
            return;
        };

        let finish = start + scaled_work(bytes, u128::from(weight));
        connection.head_tags = Some(PacketTags { start, finish });
    }

    fn min_dispatchable_start(&mut self) -> Option<u128> {
        let result = self.min_dispatchable_start_heap();
        #[cfg(debug_assertions)]
        {
            let linear = self.min_dispatchable_start_linear();
            debug_assert_eq!(
                result, linear,
                "start_heap diverged from the linear min_dispatchable_start scan"
            );
        }
        result
    }

    /// Whether a heap entry still reflects its flow's current candidate (the
    /// flow exists and its generation matches the entry's snapshot).
    fn entry_is_current(&self, entry: &CandidateEntry) -> bool {
        self.flows
            .get(&entry.flow_id)
            .is_some_and(|flow| flow.generation == entry.generation)
    }

    /// Heap-backed `min_dispatchable_start`: pop superseded entries off the top
    /// (generation no longer matching the flow), then the top is the smallest
    /// current candidate start. O(log flows) amortized.
    fn min_dispatchable_start_heap(&mut self) -> Option<u128> {
        while let Some(Reverse(entry)) = self.start_heap.peek() {
            if self.entry_is_current(&entry.0) {
                return Some(entry.0.start);
            }
            self.start_heap.pop();
        }
        None
    }

    /// Read-only linear `min_dispatchable_start`, the debug-build cross-check
    /// oracle for `start_heap`. Computes candidate starts inline without caching
    /// or mutating, so it can run beside the heap path without perturbing it.
    #[cfg(debug_assertions)]
    fn min_dispatchable_start_linear(&self) -> Option<u128> {
        let mut min_start: Option<u128> = None;
        for flow in self.flows.values() {
            let Some(selection) = Self::dispatchable_connection_for_flow(&self.connections, flow)
            else {
                continue;
            };
            let Some(connection) = self.connections.get(&selection.connection_id) else {
                continue;
            };
            let start = if let Some(tags) = connection.head_tags {
                tags.start
            } else if connection.front_packet().is_some() {
                flow.last_finish.max(self.virtual_time)
            } else {
                continue;
            };
            min_start = Some(min_start.map_or(start, |current| current.min(start)));
        }
        min_start
    }

    fn total_flow_weight_sum(&self) -> u128 {
        self.total_flow_weight_sum.max(1)
    }

    fn update_virtual_time_floor(&mut self) {
        if let Some(min_start) = self.min_dispatchable_start() {
            self.virtual_time = self.virtual_time.max(min_start);
        }
    }

    fn flow_has_dispatchable_connection(&self, flow_id: FlowId) -> bool {
        self.flows
            .get(&flow_id)
            .and_then(|flow| Self::dispatchable_connection_for_flow(&self.connections, flow))
            .is_some()
    }

    fn update_existing_flow_weight(&mut self, flow_id: FlowId, weight: u16) -> Option<bool> {
        let flow = self.flows.get_mut(&flow_id)?;

        let weight_changed = flow.weight != weight;
        if weight_changed {
            self.total_flow_weight_sum = self
                .total_flow_weight_sum
                .saturating_sub(u128::from(flow.weight))
                + u128::from(weight);
            flow.weight = weight;
        }
        flow.last_finish = flow.last_finish.max(self.virtual_time);
        Some(weight_changed)
    }

    fn remove_flow_member(
        &mut self,
        flow_id: FlowId,
        class: ConnectionClass,
        connection_id: ConnectionId,
        clear_remaining_tags: bool,
    ) {
        let should_remove = if let Some(flow) = self.flows.get_mut(&flow_id) {
            flow.remove_connection(class, connection_id);
            flow.is_empty()
        } else {
            false
        };

        if should_remove {
            // Drop this flow's dispatchable-count contribution while its flag is
            // still readable, then remove it.
            self.release_flow_dispatchable(flow_id);
            if let Some(flow) = self.flows.remove(&flow_id) {
                self.total_flow_weight_sum = self
                    .total_flow_weight_sum
                    .saturating_sub(u128::from(flow.weight));
            }
        } else {
            if clear_remaining_tags {
                self.clear_flow_tags(flow_id);
            }
            self.refresh_flow_candidate(flow_id);
        }
    }

    fn clear_flow_tags(&mut self, flow_id: FlowId) {
        let flows = &self.flows;
        let connections = &mut self.connections;
        let Some(flow) = flows.get(&flow_id) else {
            return;
        };

        for connection_id in flow
            .protocol_connections
            .iter()
            .chain(flow.bulk_connections.iter())
        {
            if let Some(connection) = connections.get_mut(connection_id) {
                connection.clear_tags();
            }
        }
    }
}

struct FlowState {
    weight: u16,
    last_finish: u128,
    protocol_connections: Vec<ConnectionId>,
    bulk_connections: Vec<ConnectionId>,
    protocol_cursor: usize,
    bulk_cursor: usize,
    /// Whether this flow currently has a dispatchable connection. Mirrors its
    /// contribution to the scheduler's `dispatchable_count`.
    dispatchable: bool,
    /// Bumped on every `refresh_flow_candidate`. A `start_heap` entry is current
    /// only while it carries this exact value; older entries are stale.
    generation: u64,
}

impl FlowState {
    fn new(weight: u16, virtual_time: u128) -> Self {
        Self {
            weight,
            last_finish: virtual_time,
            protocol_connections: Vec::new(),
            bulk_connections: Vec::new(),
            protocol_cursor: 0,
            bulk_cursor: 0,
            dispatchable: false,
            generation: 0,
        }
    }

    fn add_connection(&mut self, class: ConnectionClass, connection_id: ConnectionId) {
        match class {
            ConnectionClass::Protocol => self.protocol_connections.push(connection_id),
            ConnectionClass::Bulk => self.bulk_connections.push(connection_id),
        }
    }

    fn remove_connection(&mut self, class: ConnectionClass, connection_id: ConnectionId) {
        let (connections, cursor) = match class {
            ConnectionClass::Protocol => {
                (&mut self.protocol_connections, &mut self.protocol_cursor)
            }
            ConnectionClass::Bulk => (&mut self.bulk_connections, &mut self.bulk_cursor),
        };

        if let Some(idx) = connections.iter().position(|id| *id == connection_id) {
            connections.remove(idx);
            if !connections.is_empty() {
                *cursor %= connections.len();
            } else {
                *cursor = 0;
            }
        }
    }

    fn advance_cursor(
        &mut self,
        class: ConnectionClass,
        member_index: usize,
        connection_id: ConnectionId,
    ) {
        let (connections, cursor) = match class {
            ConnectionClass::Protocol => (&self.protocol_connections, &mut self.protocol_cursor),
            ConnectionClass::Bulk => (&self.bulk_connections, &mut self.bulk_cursor),
        };

        if connections.is_empty() {
            *cursor = 0;
            return;
        }

        debug_assert_eq!(connections.get(member_index), Some(&connection_id));
        *cursor = (member_index + 1) % connections.len();
    }

    fn is_empty(&self) -> bool {
        self.protocol_connections.is_empty() && self.bulk_connections.is_empty()
    }
}

struct ConnectionState<T> {
    flow_id: FlowId,
    class: ConnectionClass,
    blocked: bool,
    in_flight: bool,
    head_tags: Option<PacketTags>,
    priority_queue: VecDeque<QueuedPacket<T>>,
    queue: VecDeque<QueuedPacket<T>>,
    /// While a multi-chunk frame is mid-dispatch, pops are locked to the queue
    /// it came from until its final chunk, so another queue's frame cannot
    /// interleave on the wire. `None` outside a frame.
    active_queue: Option<PacketPriority>,
}

impl<T> ConnectionState<T> {
    fn clear_tags(&mut self) {
        self.head_tags = None;
    }

    fn push_packet(&mut self, packet: QueuedPacket<T>, priority: PacketPriority) -> Option<u128> {
        match priority {
            PacketPriority::Normal => {
                self.queue.push_back(packet);
                None
            }
            PacketPriority::Priority => {
                // Don't let a priority arrival preempt the head tag while a
                // normal frame is mid-dispatch — the head is locked to that
                // frame, so the priority packet waits for the frame boundary.
                let preempted_start = if self.active_queue.is_none()
                    && self.priority_queue.is_empty()
                    && !self.queue.is_empty()
                {
                    self.head_tags.take().map(|tags| tags.start)
                } else {
                    None
                };
                self.priority_queue.push_back(packet);
                preempted_start
            }
        }
    }

    /// Which queue the next packet must be served from. While a frame is
    /// mid-dispatch, pops are locked to its queue and never fall back to the
    /// other one: if the locked queue is momentarily empty the connection
    /// reports nothing servable and waits for the frame to continue, so a
    /// priority frame can never interleave inside a normal frame regardless of
    /// how a caller enqueues chunks. Outside a frame, priority-first.
    fn select_queue(&self) -> Option<PacketPriority> {
        match self.active_queue {
            Some(PacketPriority::Priority) => {
                (!self.priority_queue.is_empty()).then_some(PacketPriority::Priority)
            }
            Some(PacketPriority::Normal) => {
                (!self.queue.is_empty()).then_some(PacketPriority::Normal)
            }
            None if !self.priority_queue.is_empty() => Some(PacketPriority::Priority),
            None if !self.queue.is_empty() => Some(PacketPriority::Normal),
            None => None,
        }
    }

    fn front_packet(&self) -> Option<&QueuedPacket<T>> {
        match self.select_queue()? {
            PacketPriority::Priority => self.priority_queue.front(),
            PacketPriority::Normal => self.queue.front(),
        }
    }

    fn pop_front_packet(&mut self) -> Option<QueuedPacket<T>> {
        let from = self.select_queue()?;
        let packet = match from {
            PacketPriority::Priority => self.priority_queue.pop_front(),
            PacketPriority::Normal => self.queue.pop_front(),
        }?;
        // Lock subsequent pops to this queue until the frame's final chunk.
        self.active_queue = if packet.is_final { None } else { Some(from) };
        Some(packet)
    }

    fn is_empty(&self) -> bool {
        self.priority_queue.is_empty() && self.queue.is_empty()
    }

    fn queued_packets(&self) -> usize {
        self.priority_queue.len() + self.queue.len()
    }
}

struct QueuedPacket<T> {
    bytes: usize,
    payload: T,
    is_final: bool,
}

#[derive(Clone, Copy)]
struct PacketTags {
    start: u128,
    finish: u128,
}

#[derive(Clone, Copy)]
struct FlowSelection {
    connection_id: ConnectionId,
    class: ConnectionClass,
    member_index: usize,
}

#[derive(Clone, Copy)]
struct Candidate {
    connection_id: ConnectionId,
    class: ConnectionClass,
    member_index: usize,
}

#[derive(Clone, Copy)]
struct SelectedCandidate {
    candidate: Candidate,
}

fn normalize_weight(weight: u16) -> u16 {
    weight.max(MIN_BANDWIDTH_WEIGHT)
}

fn scaled_work(bytes: usize, weight: u128) -> u128 {
    let work = bytes as u128 * VIRTUAL_TIME_SCALE;
    work.div_ceil(weight.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKET_BYTES: usize = 1_000;

    fn conn(id: u64) -> ConnectionId {
        ConnectionId::new(id)
    }

    fn packet(id: u64) -> SchedulerPacket<u64> {
        SchedulerPacket::new(PACKET_BYTES, id)
    }

    fn sized_packet(bytes: usize, id: u64) -> SchedulerPacket<u64> {
        SchedulerPacket::new(bytes, id)
    }

    fn priority_sized_packet(bytes: usize, id: u64) -> SchedulerPacket<u64> {
        SchedulerPacket::priority(bytes, id)
    }

    fn enqueue_many(scheduler: &mut Wf2qScheduler<u64>, connection_id: ConnectionId, count: u64) {
        for id in 0..count {
            scheduler.enqueue(connection_id, packet(id)).unwrap();
        }
    }

    fn enqueue_many_sized(
        scheduler: &mut Wf2qScheduler<u64>,
        connection_id: ConnectionId,
        bytes: usize,
        count: u64,
    ) {
        for id in 0..count {
            scheduler
                .enqueue(connection_id, sized_packet(bytes, id))
                .unwrap();
        }
    }

    fn dequeue_and_ack(scheduler: &mut Wf2qScheduler<u64>) -> DequeuedPacket<u64> {
        let packet = scheduler.dequeue().unwrap();
        assert!(scheduler.set_connection_in_flight(packet.connection_id, false));
        packet
    }

    fn head_has_tags(scheduler: &Wf2qScheduler<u64>, connection_id: ConnectionId) -> bool {
        scheduler
            .connections
            .get(&connection_id)
            .is_some_and(|connection| connection.head_tags.is_some())
    }

    fn head_tags(
        scheduler: &Wf2qScheduler<u64>,
        connection_id: ConnectionId,
    ) -> Option<PacketTags> {
        scheduler
            .connections
            .get(&connection_id)
            .and_then(|connection| connection.head_tags)
    }

    fn head_finish(scheduler: &Wf2qScheduler<u64>, connection_id: ConnectionId) -> Option<u128> {
        head_tags(scheduler, connection_id).map(|tags| tags.finish)
    }

    // Broadcast-scaling benchmark. Times staging (N enqueues) and dispatch (N
    // dequeues) separately to isolate the two O(N²) sources. Ignored by default;
    // run with: cargo test -p nexus-server scheduler::tests::bench_broadcast -- --ignored --nocapture
    #[test]
    #[ignore = "benchmark"]
    fn bench_broadcast_scaling() {
        for n in [100u64, 250, 500, 1000, 2000] {
            let mut scheduler = Wf2qScheduler::new();
            for i in 0..n {
                scheduler.register_user_connection(
                    conn(i),
                    i as i64,
                    ConnectionClass::Protocol,
                    100,
                );
            }

            // Stage one frame per flow (a broadcast to N users).
            let t_stage = std::time::Instant::now();
            for i in 0..n {
                scheduler.enqueue(conn(i), packet(i)).unwrap();
            }
            let stage = t_stage.elapsed();

            // Drain all N, acking each like the writer integration does.
            let t_dispatch = std::time::Instant::now();
            let mut dispatched = 0u64;
            while let Some(pkt) = scheduler.dequeue() {
                scheduler.set_connection_in_flight(pkt.connection_id, false);
                dispatched += 1;
            }
            let dispatch = t_dispatch.elapsed();
            assert_eq!(dispatched, n);

            println!(
                "N={n:>5}  stage={stage:>12.2?}  dispatch={dispatch:>12.2?}  total={:>12.2?}",
                stage + dispatch
            );
        }
    }

    /// Tiny deterministic PRNG so the randomized harness replays identically.
    struct Lcg(u64);
    impl Lcg {
        fn new(seed: u64) -> Self {
            Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
        }
        fn next_u64(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next_u64() % n
        }
    }

    /// Drive a random op-sequence through the scheduler and return the ordered
    /// list of dispatched payloads. Deterministic for a given seed.
    fn run_randomized_workload(seed: u64) -> Vec<u64> {
        const CONNS: u64 = 8;
        const FLOWS: i64 = 4;

        let mut rng = Lcg::new(seed);
        let mut scheduler = Wf2qScheduler::new();
        for i in 0..CONNS {
            let class = if i % 2 == 0 {
                ConnectionClass::Protocol
            } else {
                ConnectionClass::Bulk
            };
            scheduler.register_user_connection(
                conn(i),
                (i as i64) % FLOWS,
                class,
                ((i % 3) + 1) as u16,
            );
        }

        let mut dispatched = Vec::new();
        let mut next_payload = 0u64;

        for _ in 0..3000 {
            match rng.below(10) {
                0..=4 => {
                    let c = conn(rng.below(CONNS));
                    if scheduler.enqueue(c, packet(next_payload)).is_ok() {
                        next_payload += 1;
                    }
                }
                5..=7 => {
                    if let Some(pkt) = scheduler.dequeue() {
                        dispatched.push(pkt.payload);
                        // Ack immediately, as the writer integration does.
                        scheduler.set_connection_in_flight(pkt.connection_id, false);
                    }
                }
                8 => {
                    let c = conn(rng.below(CONNS));
                    scheduler.set_connection_blocked(c, rng.below(2) == 0);
                }
                _ => {
                    scheduler.update_user_weight(
                        rng.below(FLOWS as u64) as i64,
                        (rng.below(3) + 1) as u16,
                    );
                }
            }
        }

        // Unblock everything and drain so conservation can be checked.
        for i in 0..CONNS {
            scheduler.set_connection_blocked(conn(i), false);
        }
        while let Some(pkt) = scheduler.dequeue() {
            dispatched.push(pkt.payload);
            scheduler.set_connection_in_flight(pkt.connection_id, false);
        }

        assert_eq!(
            next_payload as usize,
            dispatched.len(),
            "seed {seed}: every enqueued packet must dispatch exactly once"
        );
        let mut sorted = dispatched.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            dispatched.len(),
            "seed {seed}: no payload may be dispatched twice"
        );

        dispatched
    }

    #[test]
    fn randomized_workload_conserves_packets_and_is_deterministic() {
        for seed in 0..64u64 {
            let first = run_randomized_workload(seed);
            let second = run_randomized_workload(seed);
            assert_eq!(
                first, second,
                "seed {seed}: dispatch order must be deterministic"
            );
        }
    }

    #[test]
    fn refresh_churn_without_dispatch_keeps_heaps_bounded() {
        let mut scheduler = Wf2qScheduler::new();
        let c = conn(1);
        scheduler.register_user_connection(c, 1, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, c, 1);

        // Mutate repeatedly without ever dispatching or querying. Each refresh
        // pushes fresh heap entries and orphans the prior generation; lazy
        // stale-skip never runs, so only compaction can keep these bounded.
        for w in 0..10_000u16 {
            scheduler.update_user_weight(1, (w % 100) + 1);
        }

        // One dispatchable flow → at most one live entry per heap; compaction
        // caps length near the threshold rather than the 10_000-iteration churn.
        let bound = HEAP_COMPACT_FACTOR + HEAP_COMPACT_SLACK + 1;
        assert!(
            scheduler.start_heap.len() <= bound,
            "start_heap unbounded: {}",
            scheduler.start_heap.len()
        );
        assert!(
            scheduler.pending_heap.len() <= bound,
            "pending_heap unbounded: {}",
            scheduler.pending_heap.len()
        );

        // Still dispatches correctly after all the churn.
        assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, c);
    }

    #[test]
    fn weighted_flows_receive_proportional_service_while_backlogged() {
        let mut scheduler = Wf2qScheduler::new();
        let high = conn(1);
        let low = conn(2);
        scheduler.register_user_connection(high, 1, ConnectionClass::Protocol, 10);
        scheduler.register_user_connection(low, 2, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, high, 200);
        enqueue_many(&mut scheduler, low, 200);

        let mut high_count = 0;
        let mut low_count = 0;
        for _ in 0..110 {
            match dequeue_and_ack(&mut scheduler).connection_id {
                id if id == high => high_count += 1,
                id if id == low => low_count += 1,
                _ => unreachable!(),
            }
        }

        assert!(high_count >= low_count * 8);
        assert!(high_count <= low_count * 12);
    }

    #[test]
    fn variable_packet_sizes_are_byte_fair_not_packet_fair() {
        let mut scheduler = Wf2qScheduler::new();
        let small = conn(1);
        let large = conn(2);
        const SMALL_BYTES: usize = 1_000;
        const LARGE_BYTES: usize = 2_000;
        scheduler.register_user_connection(small, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(large, 2, ConnectionClass::Protocol, 1);
        enqueue_many_sized(&mut scheduler, small, SMALL_BYTES, 300);
        enqueue_many_sized(&mut scheduler, large, LARGE_BYTES, 300);

        let mut small_packets = 0;
        let mut small_bytes = 0;
        let mut large_packets = 0;
        let mut large_bytes = 0;
        for _ in 0..150 {
            let dequeued = dequeue_and_ack(&mut scheduler);
            match dequeued.connection_id {
                id if id == small => {
                    small_packets += 1;
                    small_bytes += dequeued.bytes;
                }
                id if id == large => {
                    large_packets += 1;
                    large_bytes += dequeued.bytes;
                }
                _ => unreachable!(),
            }
        }

        assert!(small_packets > large_packets);
        assert!(small_bytes.abs_diff(large_bytes) <= LARGE_BYTES);
    }

    #[test]
    fn continuously_backlogged_flow_tags_next_head_from_previous_finish() {
        let mut scheduler = Wf2qScheduler::new();
        let connection = conn(1);
        scheduler.register_user_connection(connection, 1, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, connection, 2);

        assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, connection);

        let tags = head_tags(&scheduler, connection).unwrap();
        let first_finish = scaled_work(PACKET_BYTES, 1);
        assert_eq!(tags.start, first_finish);
        assert_eq!(tags.finish, first_finish + scaled_work(PACKET_BYTES, 1));
    }

    #[test]
    fn priority_preemption_retains_flow_tag_budget_for_displaced_normal_head() {
        let mut scheduler = Wf2qScheduler::new();
        let connection = conn(1);
        scheduler.register_user_connection(connection, 1, ConnectionClass::Protocol, 1);

        scheduler.enqueue(connection, sized_packet(100, 1)).unwrap();
        let normal_tags = head_tags(&scheduler, connection).unwrap();

        scheduler
            .enqueue(connection, priority_sized_packet(20, 2))
            .unwrap();
        let priority_tags = head_tags(&scheduler, connection).unwrap();

        assert_eq!(priority_tags.start, normal_tags.start);
        assert_eq!(
            priority_tags.finish,
            priority_tags.start + scaled_work(20, 1)
        );

        let priority = dequeue_and_ack(&mut scheduler);
        assert_eq!(priority.payload, 2);

        let displaced_normal_tags = head_tags(&scheduler, connection).unwrap();
        assert_eq!(displaced_normal_tags.start, priority_tags.finish);
        assert_eq!(
            displaced_normal_tags.finish,
            priority_tags.finish + scaled_work(100, 1)
        );

        let normal = dequeue_and_ack(&mut scheduler);
        assert_eq!(normal.payload, 1);
    }

    #[test]
    fn priority_preemption_preserves_cross_flow_fairness() {
        let mut scheduler = Wf2qScheduler::new();
        let priority_flow = conn(1);
        let other_flow = conn(2);
        scheduler.register_user_connection(priority_flow, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(other_flow, 2, ConnectionClass::Protocol, 1);

        enqueue_many(&mut scheduler, priority_flow, 200);
        enqueue_many(&mut scheduler, other_flow, 200);
        scheduler
            .enqueue(priority_flow, priority_sized_packet(PACKET_BYTES, 999))
            .unwrap();

        let mut priority_flow_count = 0usize;
        let mut other_flow_count = 0usize;
        let mut saw_priority_payload = false;
        for _ in 0..200 {
            let packet = dequeue_and_ack(&mut scheduler);
            match packet.connection_id {
                id if id == priority_flow => {
                    priority_flow_count += 1;
                    saw_priority_payload |= packet.payload == 999;
                }
                id if id == other_flow => other_flow_count += 1,
                _ => panic!("unexpected connection"),
            }
        }

        assert!(saw_priority_payload);
        assert!(priority_flow_count.abs_diff(other_flow_count) <= 2);
    }

    #[test]
    fn enqueue_to_idle_flow_updates_virtual_time_before_later_arrivals() {
        let mut scheduler = Wf2qScheduler::new();
        let first = conn(1);
        let second = conn(2);
        scheduler.register_user_connection(first, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(second, 2, ConnectionClass::Protocol, 1);

        let work = scaled_work(PACKET_BYTES, 1);
        let first_last_finish = work * 4;
        scheduler.virtual_time = work;
        scheduler
            .flows
            .get_mut(&FlowId::User(1))
            .unwrap()
            .last_finish = first_last_finish;
        scheduler
            .flows
            .get_mut(&FlowId::User(2))
            .unwrap()
            .last_finish = work;

        scheduler.enqueue(first, packet(1)).unwrap();
        assert_eq!(scheduler.virtual_time, first_last_finish);

        scheduler.enqueue(second, packet(2)).unwrap();
        assert_eq!(
            head_tags(&scheduler, first).unwrap().start,
            first_last_finish
        );
        assert_eq!(
            head_tags(&scheduler, second).unwrap().start,
            first_last_finish
        );
    }

    #[test]
    fn anon_connections_share_one_global_flow() {
        let mut scheduler = Wf2qScheduler::new();
        let first_anon = conn(1);
        let second_anon = conn(2);
        let user = conn(3);
        scheduler.register_anon_connection(first_anon, ConnectionClass::Protocol);
        scheduler.register_anon_connection(second_anon, ConnectionClass::Protocol);
        scheduler.register_user_connection(user, 1, ConnectionClass::Protocol, ANON_FLOW_WEIGHT);
        enqueue_many(&mut scheduler, first_anon, 200);
        enqueue_many(&mut scheduler, second_anon, 200);
        enqueue_many(&mut scheduler, user, 200);

        let mut first_anon_count: usize = 0;
        let mut second_anon_count: usize = 0;
        let mut user_count: usize = 0;
        for _ in 0..120 {
            match dequeue_and_ack(&mut scheduler).connection_id {
                id if id == first_anon => first_anon_count += 1,
                id if id == second_anon => second_anon_count += 1,
                id if id == user => user_count += 1,
                _ => unreachable!(),
            }
        }

        let anon_total = first_anon_count + second_anon_count;
        assert!(anon_total.abs_diff(user_count) <= 2);
        assert!(first_anon_count.abs_diff(second_anon_count) <= 1);
    }

    #[test]
    fn user_flow_share_is_not_multiplied_by_multiple_connections() {
        let mut scheduler = Wf2qScheduler::new();
        let first_user_conn = conn(1);
        let second_user_conn = conn(2);
        let other_user_conn = conn(3);
        scheduler.register_user_connection(first_user_conn, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(second_user_conn, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(other_user_conn, 2, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, first_user_conn, 200);
        enqueue_many(&mut scheduler, second_user_conn, 200);
        enqueue_many(&mut scheduler, other_user_conn, 200);

        let mut first_user_conn_count: usize = 0;
        let mut second_user_conn_count: usize = 0;
        let mut other_user_conn_count: usize = 0;
        for _ in 0..120 {
            match dequeue_and_ack(&mut scheduler).connection_id {
                id if id == first_user_conn => first_user_conn_count += 1,
                id if id == second_user_conn => second_user_conn_count += 1,
                id if id == other_user_conn => other_user_conn_count += 1,
                _ => unreachable!(),
            }
        }

        let first_user_total = first_user_conn_count + second_user_conn_count;
        assert!(first_user_total.abs_diff(other_user_conn_count) <= 2);
        assert!(first_user_conn_count.abs_diff(second_user_conn_count) <= 1);
    }

    #[test]
    fn mixed_class_connections_still_share_one_user_flow() {
        let mut scheduler = Wf2qScheduler::new();
        let protocol = conn(1);
        let bulk = conn(2);
        let other = conn(3);
        scheduler.register_user_connection(protocol, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(bulk, 1, ConnectionClass::Bulk, 1);
        scheduler.register_user_connection(other, 2, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, protocol, 30);
        enqueue_many(&mut scheduler, bulk, 200);
        enqueue_many(&mut scheduler, other, 230);

        let mut protocol_count: usize = 0;
        let mut bulk_count: usize = 0;
        let mut other_count: usize = 0;
        let mut saw_user_bulk = false;
        for _ in 0..120 {
            let dequeued = dequeue_and_ack(&mut scheduler);
            match dequeued.connection_id {
                id if id == protocol => {
                    assert!(!saw_user_bulk);
                    protocol_count += 1;
                }
                id if id == bulk => {
                    saw_user_bulk = true;
                    bulk_count += 1;
                }
                id if id == other => other_count += 1,
                _ => unreachable!(),
            }
        }

        let user_total = protocol_count + bulk_count;
        assert_eq!(protocol_count, 30);
        assert!(bulk_count > 0);
        assert!(user_total.abs_diff(other_count) <= 2);
    }

    #[test]
    fn blocked_and_idle_registered_flows_do_not_distort_active_fairness() {
        let mut scheduler = Wf2qScheduler::new();
        let first_active = conn(1);
        let second_active = conn(2);
        scheduler.register_user_connection(first_active, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(second_active, 2, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, first_active, 200);
        enqueue_many(&mut scheduler, second_active, 200);

        let blocked_connections: Vec<_> = (3..13).map(conn).collect();
        for (idx, connection_id) in blocked_connections.iter().enumerate() {
            scheduler.register_user_connection(
                *connection_id,
                idx as i64 + 3,
                ConnectionClass::Protocol,
                ANON_FLOW_WEIGHT,
            );
            enqueue_many(&mut scheduler, *connection_id, 200);
            scheduler.set_connection_blocked(*connection_id, true);
        }

        for id in 20..30 {
            scheduler.register_user_connection(
                conn(id),
                i64::try_from(id).unwrap(),
                ConnectionClass::Protocol,
                ANON_FLOW_WEIGHT,
            );
        }

        let mut first_active_count: usize = 0;
        let mut second_active_count: usize = 0;
        for _ in 0..120 {
            match dequeue_and_ack(&mut scheduler).connection_id {
                id if id == first_active => first_active_count += 1,
                id if id == second_active => second_active_count += 1,
                id if blocked_connections.contains(&id) => {
                    panic!("blocked connection was dispatched")
                }
                _ => unreachable!(),
            }
        }

        assert!(first_active_count.abs_diff(second_active_count) <= 2);
    }

    #[test]
    fn update_user_weight_clears_cached_tags_and_changes_service_share() {
        let mut scheduler = Wf2qScheduler::new();
        let baseline = conn(1);
        let boosted = conn(2);
        scheduler.register_user_connection(baseline, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(boosted, 2, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, baseline, 200);
        enqueue_many(&mut scheduler, boosted, 200);

        assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, baseline);
        let before = head_finish(&scheduler, boosted).expect("boosted head tags");

        assert!(scheduler.update_user_weight(2, 10));
        // The weight change recomputes the head tags (eager) with the new
        // weight, so the finish reflects the boost rather than the stale value.
        assert_ne!(head_finish(&scheduler, boosted), Some(before));

        let mut baseline_count = 0;
        let mut boosted_count = 0;
        for _ in 0..110 {
            match dequeue_and_ack(&mut scheduler).connection_id {
                id if id == baseline => baseline_count += 1,
                id if id == boosted => boosted_count += 1,
                _ => unreachable!(),
            }
        }

        assert!(boosted_count >= baseline_count * 8);
        assert!(boosted_count <= baseline_count * 12);
    }

    #[test]
    fn update_user_weight_decrease_clears_cached_tags_and_changes_service_share() {
        let mut scheduler = Wf2qScheduler::new();
        let baseline = conn(1);
        let reduced = conn(2);
        scheduler.register_user_connection(baseline, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(reduced, 2, ConnectionClass::Protocol, 10);
        enqueue_many(&mut scheduler, baseline, 200);
        enqueue_many(&mut scheduler, reduced, 200);

        assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, reduced);
        assert!(head_has_tags(&scheduler, baseline));
        let before = head_finish(&scheduler, reduced).expect("reduced head tags");

        assert!(scheduler.update_user_weight(2, 1));
        // The weight change recomputes the head tags (eager) with the new
        // weight, so the finish reflects the reduction rather than the stale value.
        assert_ne!(head_finish(&scheduler, reduced), Some(before));

        let mut baseline_count: usize = 0;
        let mut reduced_count: usize = 0;
        for _ in 0..120 {
            match dequeue_and_ack(&mut scheduler).connection_id {
                id if id == baseline => baseline_count += 1,
                id if id == reduced => reduced_count += 1,
                _ => unreachable!(),
            }
        }

        assert!(baseline_count.abs_diff(reduced_count) <= 2);
    }

    #[test]
    fn registering_existing_flow_with_changed_weight_clears_cached_tags() {
        let mut scheduler = Wf2qScheduler::new();
        let first = conn(1);
        let second = conn(2);
        scheduler.register_user_connection(first, 1, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, first, 1);

        assert!(scheduler.select_candidate().is_some());
        let before = head_finish(&scheduler, first).expect("first head tags");

        assert!(scheduler.register_user_connection(second, 1, ConnectionClass::Protocol, 10));
        assert_eq!(scheduler.total_flow_weight_sum, 10);
        assert_eq!(scheduler.flows.get(&FlowId::User(1)).unwrap().weight, 10);
        // Registering a heavier sibling changes the flow weight, recomputing the
        // head tags (eager) to reflect it.
        assert_ne!(head_finish(&scheduler, first), Some(before));
    }

    #[test]
    fn protocol_class_runs_before_bulk_within_same_flow() {
        let mut scheduler = Wf2qScheduler::new();
        let protocol = conn(1);
        let bulk = conn(2);
        scheduler.register_user_connection(protocol, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(bulk, 1, ConnectionClass::Bulk, 1);
        enqueue_many(&mut scheduler, bulk, 3);
        enqueue_many(&mut scheduler, protocol, 3);

        for _ in 0..3 {
            let dequeued = dequeue_and_ack(&mut scheduler);
            assert_eq!(dequeued.connection_id, protocol);
            assert_eq!(dequeued.class, ConnectionClass::Protocol);
        }

        for _ in 0..3 {
            let dequeued = dequeue_and_ack(&mut scheduler);
            assert_eq!(dequeued.connection_id, bulk);
            assert_eq!(dequeued.class, ConnectionClass::Bulk);
        }
    }

    #[test]
    fn blocked_protocol_connection_does_not_hold_back_bulk() {
        let mut scheduler = Wf2qScheduler::new();
        let protocol = conn(1);
        let bulk = conn(2);
        scheduler.register_user_connection(protocol, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(bulk, 1, ConnectionClass::Bulk, 1);
        enqueue_many(&mut scheduler, protocol, 1);
        enqueue_many(&mut scheduler, bulk, 1);
        scheduler.set_connection_blocked(protocol, true);

        let dequeued = dequeue_and_ack(&mut scheduler);
        assert_eq!(dequeued.connection_id, bulk);
        assert_eq!(dequeued.class, ConnectionClass::Bulk);
    }

    #[test]
    fn round_robins_within_the_selected_class() {
        let mut scheduler = Wf2qScheduler::new();
        let first = conn(1);
        let second = conn(2);
        scheduler.register_user_connection(first, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(second, 1, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, first, 2);
        enqueue_many(&mut scheduler, second, 2);

        assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, first);
        assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, second);
        assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, first);
        assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, second);
    }

    #[test]
    fn blocked_flow_does_not_reserve_rate_or_receive_catchup_burst() {
        let mut scheduler = Wf2qScheduler::new();
        let blocked = conn(1);
        let active = conn(2);
        scheduler.register_user_connection(blocked, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(active, 2, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, blocked, 20);
        enqueue_many(&mut scheduler, active, 20);
        scheduler.set_connection_blocked(blocked, true);

        for _ in 0..5 {
            assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, active);
        }

        scheduler.set_connection_blocked(blocked, false);

        let next: Vec<_> = (0..4)
            .map(|_| dequeue_and_ack(&mut scheduler).connection_id)
            .collect();
        assert_eq!(next, vec![blocked, active, blocked, active]);
    }

    #[test]
    fn in_flight_connection_is_not_dispatched_again_until_acked() {
        let mut scheduler = Wf2qScheduler::new();
        let connection = conn(1);
        scheduler.register_user_connection(connection, 1, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, connection, 2);

        assert_eq!(scheduler.dequeue().unwrap().connection_id, connection);
        assert!(scheduler.dequeue().is_none());

        assert!(scheduler.set_connection_in_flight(connection, false));
        assert_eq!(scheduler.dequeue().unwrap().connection_id, connection);
    }

    #[test]
    fn in_flight_state_preserves_cached_tags() {
        let mut scheduler = Wf2qScheduler::new();
        let connection = conn(1);
        scheduler.register_user_connection(connection, 1, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, connection, 2);

        assert_eq!(scheduler.dequeue().unwrap().connection_id, connection);
        let tags_before = head_tags(&scheduler, connection).unwrap();

        let tags_during = head_tags(&scheduler, connection).unwrap();
        assert_eq!(tags_during.start, tags_before.start);
        assert_eq!(tags_during.finish, tags_before.finish);

        assert!(scheduler.set_connection_in_flight(connection, false));
        let tags_after = head_tags(&scheduler, connection).unwrap();
        assert_eq!(tags_after.start, tags_before.start);
        assert_eq!(tags_after.finish, tags_before.finish);
        assert_eq!(scheduler.dequeue().unwrap().connection_id, connection);
    }

    #[test]
    fn in_flight_connection_does_not_block_same_user_sibling_connection() {
        let mut scheduler = Wf2qScheduler::new();
        let in_flight = conn(1);
        let sibling = conn(2);
        scheduler.register_user_connection(in_flight, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(sibling, 1, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, in_flight, 2);
        enqueue_many(&mut scheduler, sibling, 2);

        assert_eq!(scheduler.dequeue().unwrap().connection_id, in_flight);

        assert_eq!(scheduler.dequeue().unwrap().connection_id, sibling);
    }

    #[test]
    fn priority_frame_does_not_interleave_inside_a_normal_frame() {
        let mut scheduler = Wf2qScheduler::new();
        let connection = conn(1);
        scheduler.register_user_connection(connection, 1, ConnectionClass::Protocol, 1);

        // A two-chunk normal frame: chunk 1 is non-final, chunk 2 completes it.
        let mut chunk1 = sized_packet(PACKET_BYTES, 1);
        chunk1.is_final = false;
        scheduler.enqueue(connection, chunk1).unwrap();
        scheduler
            .enqueue(connection, sized_packet(PACKET_BYTES, 2))
            .unwrap();

        // Dispatch chunk 1 of the normal frame.
        assert_eq!(scheduler.dequeue().unwrap().payload, 1);
        assert!(scheduler.set_connection_in_flight(connection, false));

        // A priority frame is staged while the normal frame is mid-dispatch.
        scheduler
            .enqueue(connection, priority_sized_packet(PACKET_BYTES, 99))
            .unwrap();

        // The next dispatch must finish the normal frame (chunk 2), not let the
        // priority frame interleave inside it.
        assert_eq!(
            scheduler.dequeue().unwrap().payload,
            2,
            "priority frame must not interleave inside a partially dispatched normal frame"
        );
        assert!(scheduler.set_connection_in_flight(connection, false));

        // At the frame boundary, the priority frame now dispatches.
        assert_eq!(scheduler.dequeue().unwrap().payload, 99);
    }

    #[test]
    fn lock_holds_when_the_normal_queue_drains_before_the_final_chunk() {
        // EgressManager enqueues a frame's chunks contiguously, but the scheduler
        // invariant must hold even if a caller enqueues them incrementally: while
        // locked to the normal queue, a momentarily-empty queue means "wait for
        // the frame to continue", never "serve the priority queue".
        let mut scheduler = Wf2qScheduler::new();
        let connection = conn(1);
        scheduler.register_user_connection(connection, 1, ConnectionClass::Protocol, 1);

        // Dispatch a non-final first chunk, leaving the normal queue empty.
        let mut chunk1 = sized_packet(PACKET_BYTES, 1);
        chunk1.is_final = false;
        scheduler.enqueue(connection, chunk1).unwrap();
        assert_eq!(scheduler.dequeue().unwrap().payload, 1);
        assert!(scheduler.set_connection_in_flight(connection, false));

        // A priority frame arrives while the connection is locked to the now-empty
        // normal queue.
        scheduler
            .enqueue(connection, priority_sized_packet(PACKET_BYTES, 99))
            .unwrap();

        // The lock holds: nothing dispatches until the normal frame continues.
        assert!(scheduler.dequeue().is_none());

        // The normal frame's final chunk arrives and dispatches first.
        scheduler
            .enqueue(connection, sized_packet(PACKET_BYTES, 2))
            .unwrap();
        assert_eq!(scheduler.dequeue().unwrap().payload, 2);
        assert!(scheduler.set_connection_in_flight(connection, false));

        // Only now, at the frame boundary, does the priority frame dispatch.
        assert_eq!(scheduler.dequeue().unwrap().payload, 99);
    }

    #[test]
    fn repeated_in_flight_cycles_preserve_fair_share() {
        let mut scheduler = Wf2qScheduler::new();
        let first = conn(1);
        let second = conn(2);
        scheduler.register_user_connection(first, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(second, 2, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, first, 100);
        enqueue_many(&mut scheduler, second, 100);

        let mut first_count: usize = 0;
        let mut second_count: usize = 0;
        for _ in 0..120 {
            let dequeued = scheduler.dequeue().unwrap();
            match dequeued.connection_id {
                id if id == first => first_count += 1,
                id if id == second => second_count += 1,
                _ => unreachable!(),
            }
            assert!(scheduler.set_connection_in_flight(dequeued.connection_id, false));
        }

        assert!(first_count.abs_diff(second_count) <= 2);
    }

    #[test]
    fn transition_while_in_flight_clears_cached_tags_and_recomputes_on_ack() {
        let mut scheduler = Wf2qScheduler::new();
        let connection = conn(1);
        scheduler.register_anon_connection(connection, ConnectionClass::Protocol);
        enqueue_many(&mut scheduler, connection, 3);

        assert_eq!(scheduler.dequeue().unwrap().connection_id, connection);
        assert!(head_has_tags(&scheduler, connection));

        assert!(scheduler.transition_to_user(connection, 42, 5));
        assert!(!head_has_tags(&scheduler, connection));

        assert!(scheduler.set_connection_in_flight(connection, false));
        assert!(scheduler.select_candidate().is_some());
        assert!(head_has_tags(&scheduler, connection));

        let dequeued = scheduler.dequeue().unwrap();
        assert_eq!(dequeued.connection_id, connection);
        assert_eq!(dequeued.flow_id, FlowId::User(42));
    }

    #[test]
    fn weight_update_while_in_flight_clears_cached_tags_and_recomputes_on_ack() {
        let mut scheduler = Wf2qScheduler::new();
        let connection = conn(1);
        scheduler.register_user_connection(connection, 1, ConnectionClass::Protocol, 10);
        enqueue_many(&mut scheduler, connection, 3);

        assert_eq!(scheduler.dequeue().unwrap().connection_id, connection);
        assert!(head_has_tags(&scheduler, connection));

        assert!(scheduler.update_user_weight(1, 1));
        assert!(!head_has_tags(&scheduler, connection));

        assert!(scheduler.set_connection_in_flight(connection, false));
        assert!(scheduler.select_candidate().is_some());
        let tags = head_tags(&scheduler, connection).unwrap();
        assert_eq!(tags.finish - tags.start, scaled_work(PACKET_BYTES, 1));
    }

    #[test]
    fn unregister_while_in_flight_then_ack_is_harmless() {
        let mut scheduler = Wf2qScheduler::new();
        let connection = conn(1);
        scheduler.register_user_connection(connection, 1, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, connection, 1);

        assert_eq!(scheduler.dequeue().unwrap().connection_id, connection);
        assert!(scheduler.unregister_connection(connection));

        assert!(!scheduler.set_connection_in_flight(connection, false));
        assert_eq!(scheduler.queued_packets(connection), None);
        assert!(scheduler.dequeue().is_none());
    }

    #[test]
    fn transition_to_user_moves_queued_traffic_to_user_flow() {
        let mut scheduler = Wf2qScheduler::new();
        let connection = conn(1);
        scheduler.register_anon_connection(connection, ConnectionClass::Protocol);
        scheduler.enqueue(connection, packet(1)).unwrap();
        assert!(scheduler.transition_to_user(connection, 42, 5));

        let dequeued = dequeue_and_ack(&mut scheduler);
        assert_eq!(dequeued.connection_id, connection);
        assert_eq!(dequeued.flow_id, FlowId::User(42));
    }

    #[test]
    fn transition_to_user_clears_cached_tags() {
        let mut scheduler = Wf2qScheduler::new();
        let transitioning = conn(1);
        let active = conn(2);
        scheduler.register_anon_connection(transitioning, ConnectionClass::Protocol);
        scheduler.register_user_connection(active, 1, ConnectionClass::Protocol, ANON_FLOW_WEIGHT);
        enqueue_many_sized(&mut scheduler, transitioning, PACKET_BYTES * 10, 20);
        enqueue_many_sized(&mut scheduler, active, PACKET_BYTES, 20);

        assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, active);
        let before = head_finish(&scheduler, transitioning).expect("transitioning head tags");

        assert!(scheduler.transition_to_user(transitioning, 2, ANON_FLOW_WEIGHT));
        // The transition moves the connection to a new flow and recomputes its
        // head tags there (eager), so the finish reflects the new timeline.
        assert_ne!(head_finish(&scheduler, transitioning), Some(before));
        assert_eq!(scheduler.queued_packets(transitioning), Some(20));
    }

    #[test]
    fn same_flow_transition_clears_sibling_cached_tags() {
        let mut scheduler = Wf2qScheduler::new();
        let sibling = conn(1);
        let transitioning = conn(2);
        scheduler.register_user_connection(sibling, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(transitioning, 1, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, sibling, 1);
        enqueue_many(&mut scheduler, transitioning, 1);

        assert!(scheduler.select_candidate().is_some());
        let before = head_finish(&scheduler, sibling).expect("sibling head tags");

        assert!(scheduler.transition_to_user(transitioning, 1, 10));
        // A same-flow transition that changes the weight recomputes the flow
        // head's tags (eager) to reflect the new weight.
        assert_ne!(head_finish(&scheduler, sibling), Some(before));
    }

    #[test]
    fn transition_to_user_after_inactive_period_does_not_catch_up() {
        let mut scheduler = Wf2qScheduler::new();
        let transitioning = conn(1);
        let active = conn(2);
        scheduler.register_anon_connection(transitioning, ConnectionClass::Protocol);
        scheduler.register_user_connection(active, 1, ConnectionClass::Protocol, ANON_FLOW_WEIGHT);
        enqueue_many(&mut scheduler, transitioning, 20);
        enqueue_many(&mut scheduler, active, 20);
        scheduler.set_connection_blocked(transitioning, true);

        for _ in 0..5 {
            assert_eq!(dequeue_and_ack(&mut scheduler).connection_id, active);
        }

        assert!(scheduler.transition_to_user(transitioning, 2, ANON_FLOW_WEIGHT));
        scheduler.set_connection_blocked(transitioning, false);

        let next: Vec<_> = (0..4)
            .map(|_| dequeue_and_ack(&mut scheduler).connection_id)
            .collect();
        assert_eq!(next.iter().filter(|&&id| id == active).count(), 2);
        assert_eq!(next.iter().filter(|&&id| id == transitioning).count(), 2);
        assert!(
            next.windows(2)
                .all(|window| window != [transitioning, transitioning])
        );
    }

    #[test]
    fn unregister_removes_connection_from_future_dispatch() {
        let mut scheduler = Wf2qScheduler::new();
        let first = conn(1);
        let second = conn(2);
        scheduler.register_user_connection(first, 1, ConnectionClass::Protocol, 1);
        scheduler.register_user_connection(second, 2, ConnectionClass::Protocol, 1);
        enqueue_many(&mut scheduler, first, 1);
        enqueue_many(&mut scheduler, second, 1);

        assert!(scheduler.unregister_connection(first));

        let dequeued = dequeue_and_ack(&mut scheduler);
        assert_eq!(dequeued.connection_id, second);
        assert!(scheduler.dequeue().is_none());
    }

    #[test]
    fn enqueue_rejects_unknown_and_empty_packets_without_dropping_payload() {
        let mut scheduler = Wf2qScheduler::new();
        let unknown = conn(1);
        let err = scheduler
            .enqueue(unknown, SchedulerPacket::new(PACKET_BYTES, 7))
            .unwrap_err();
        assert_eq!(
            err,
            EnqueueError::UnknownConnection {
                connection_id: unknown,
                packet: SchedulerPacket::new(PACKET_BYTES, 7),
            }
        );

        scheduler.register_user_connection(unknown, 1, ConnectionClass::Protocol, 1);
        let err = scheduler
            .enqueue(unknown, SchedulerPacket::new(0, 9))
            .unwrap_err();
        assert_eq!(
            err,
            EnqueueError::EmptyPacket {
                connection_id: unknown,
                packet: SchedulerPacket::new(0, 9),
            }
        );
    }
}
