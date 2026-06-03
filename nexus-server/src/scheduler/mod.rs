//! Weighted fair queuing core for future server egress scheduling.
//!
//! This module is deliberately isolated from sockets and async I/O. It owns
//! virtual-time accounting, flow membership, intra-flow priority, and blocked
//! connection skipping; the future scheduler task will wrap this core with
//! rate-budget and writer-channel integration.

use std::collections::{HashMap, VecDeque};

use nexus_common::validators::{DEFAULT_ADMIN_BANDWIDTH_WEIGHT, MIN_BANDWIDTH_WEIGHT};

const VIRTUAL_TIME_SCALE: u128 = 1_000_000;

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

/// Packet stored in the scheduler queue.
#[derive(Debug, PartialEq, Eq)]
pub struct SchedulerPacket<T> {
    pub bytes: usize,
    pub payload: T,
}

impl<T> SchedulerPacket<T> {
    pub fn new(bytes: usize, payload: T) -> Self {
        Self { bytes, payload }
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
    pending_scratch: Vec<PendingCandidate>,
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
            pending_scratch: Vec::new(),
        }
    }

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
                queue: VecDeque::new(),
            },
        );

        if should_clear_tags {
            self.clear_flow_tags(flow_id);
        }

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

        let clear_remaining_tags = !connection.queue.is_empty() || connection.head_tags.is_some();
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
            !connection.queue.is_empty() || connection.head_tags.is_some();
        if old_flow_id == new_flow_id {
            let Some(weight_changed) = self.update_existing_flow_weight(new_flow_id, weight) else {
                return false;
            };
            if weight_changed {
                self.clear_flow_tags(new_flow_id);
            }
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
        true
    }

    pub fn set_connection_blocked(&mut self, connection_id: ConnectionId, blocked: bool) -> bool {
        let Some(connection) = self.connections.get_mut(&connection_id) else {
            return false;
        };

        if connection.blocked != blocked {
            connection.blocked = blocked;
            connection.clear_tags();
        }
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
        let was_dispatchable = self.flow_has_dispatchable_connection(flow_id);

        let Some(connection) = self.connections.get_mut(&connection_id) else {
            return Err(EnqueueError::UnknownConnection {
                connection_id,
                packet,
            });
        };
        connection.queue.push_back(QueuedPacket {
            bytes: packet.bytes,
            payload: packet.payload,
        });

        if !was_dispatchable {
            self.tag_initial_dispatchable_for_flow(flow_id);
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
            .map(|connection| connection.queue.len())
    }

    pub fn has_dispatchable_packet(&self) -> bool {
        self.flows
            .values()
            .any(|flow| Self::dispatchable_connection_for_flow(&self.connections, flow).is_some())
    }

    pub fn virtual_time(&self) -> u128 {
        self.virtual_time
    }

    fn select_candidate(&mut self) -> Option<SelectedCandidate> {
        self.collect_pending_candidates();

        let mut min_start = None;
        let mut best_eligible = None;
        let mut best_after_idle = None;
        for idx in 0..self.pending_scratch.len() {
            let pending = self.pending_scratch[idx];
            if let Some(tags) = self.ensure_candidate_tags(
                pending.connection_id,
                pending.weight,
                pending.last_finish,
            ) {
                let candidate = Candidate {
                    flow_id: pending.flow_id,
                    connection_id: pending.connection_id,
                    class: pending.class,
                    member_index: pending.member_index,
                    tags,
                };

                if tags.start <= self.virtual_time {
                    keep_best_candidate(&mut best_eligible, candidate);
                }

                match min_start {
                    Some(current_min) if tags.start > current_min => {}
                    Some(current_min) if tags.start == current_min => {
                        keep_best_candidate(&mut best_after_idle, candidate);
                    }
                    _ => {
                        min_start = Some(tags.start);
                        best_after_idle = Some(candidate);
                    }
                }
            }
        }

        let candidate = if let Some(candidate) = best_eligible {
            candidate
        } else {
            self.virtual_time = min_start?;
            best_after_idle?
        };

        Some(SelectedCandidate { candidate })
    }

    fn collect_pending_candidates(&mut self) {
        let flows = &self.flows;
        let connections = &self.connections;
        let pending_scratch = &mut self.pending_scratch;

        pending_scratch.clear();
        for (flow_id, flow) in flows {
            if let Some(selection) = Self::dispatchable_connection_for_flow(connections, flow) {
                pending_scratch.push(PendingCandidate {
                    flow_id: *flow_id,
                    connection_id: selection.connection_id,
                    class: selection.class,
                    member_index: selection.member_index,
                    weight: flow.weight,
                    last_finish: flow.last_finish,
                });
            }
        }
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
                    !connection.blocked && !connection.in_flight && !connection.queue.is_empty()
                })
                .map(|_| FlowSelection {
                    connection_id,
                    class,
                    member_index: idx,
                })
        })
    }

    fn ensure_candidate_tags(
        &mut self,
        connection_id: ConnectionId,
        weight: u16,
        last_finish: u128,
    ) -> Option<PacketTags> {
        let virtual_time = self.virtual_time;

        let connection = self.connections.get_mut(&connection_id)?;
        let bytes = connection.queue.front()?.bytes;
        if connection.head_tags.is_none() {
            let start = last_finish.max(virtual_time);
            let finish = start + scaled_work(bytes, u128::from(weight));
            connection.head_tags = Some(PacketTags { start, finish });
        }

        connection.head_tags
    }

    fn dequeue_candidate(&mut self, selected: SelectedCandidate) -> Option<DequeuedPacket<T>> {
        let candidate = selected.candidate;
        let total_weight_sum = self.total_flow_weight_sum();

        let connection = self.connections.get_mut(&candidate.connection_id)?;
        let tags = connection.head_tags.take()?;
        let packet = connection.queue.pop_front()?;
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
        let Some(bytes) = connection.queue.front().map(|packet| packet.bytes) else {
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
        let Some(bytes) = connection.queue.front().map(|packet| packet.bytes) else {
            return;
        };

        let finish = start + scaled_work(bytes, u128::from(weight));
        connection.head_tags = Some(PacketTags { start, finish });
    }

    fn min_dispatchable_start(&mut self) -> Option<u128> {
        self.collect_pending_candidates();

        let mut min_start = None;
        for idx in 0..self.pending_scratch.len() {
            let pending = self.pending_scratch[idx];
            let Some(tags) = self.ensure_candidate_tags(
                pending.connection_id,
                pending.weight,
                pending.last_finish,
            ) else {
                continue;
            };
            min_start = Some(min_start.map_or(tags.start, |current: u128| current.min(tags.start)));
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
            if let Some(flow) = self.flows.remove(&flow_id) {
                self.total_flow_weight_sum = self
                    .total_flow_weight_sum
                    .saturating_sub(u128::from(flow.weight));
            }
        } else if clear_remaining_tags {
            self.clear_flow_tags(flow_id);
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
    queue: VecDeque<QueuedPacket<T>>,
}

impl<T> ConnectionState<T> {
    fn clear_tags(&mut self) {
        self.head_tags = None;
    }
}

struct QueuedPacket<T> {
    bytes: usize,
    payload: T,
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
struct PendingCandidate {
    flow_id: FlowId,
    connection_id: ConnectionId,
    class: ConnectionClass,
    member_index: usize,
    weight: u16,
    last_finish: u128,
}

#[derive(Clone, Copy)]
struct Candidate {
    flow_id: FlowId,
    connection_id: ConnectionId,
    class: ConnectionClass,
    member_index: usize,
    tags: PacketTags,
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

fn keep_best_candidate(best: &mut Option<Candidate>, candidate: Candidate) {
    let Some(current_best) = best else {
        *best = Some(candidate);
        return;
    };

    let current_key = (
        current_best.tags.finish,
        current_best.flow_id,
        current_best.connection_id,
    );
    let candidate_key = (
        candidate.tags.finish,
        candidate.flow_id,
        candidate.connection_id,
    );
    if candidate_key < current_key {
        *best = Some(candidate);
    }
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
        assert!(head_has_tags(&scheduler, boosted));

        assert!(scheduler.update_user_weight(2, 10));
        assert!(!head_has_tags(&scheduler, boosted));

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

        assert!(scheduler.update_user_weight(2, 1));
        assert!(!head_has_tags(&scheduler, reduced));

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
        assert!(head_has_tags(&scheduler, first));

        assert!(scheduler.register_user_connection(second, 1, ConnectionClass::Protocol, 10));
        assert_eq!(scheduler.total_flow_weight_sum, 10);
        assert_eq!(scheduler.flows.get(&FlowId::User(1)).unwrap().weight, 10);
        assert!(!head_has_tags(&scheduler, first));
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
        assert!(head_has_tags(&scheduler, transitioning));

        assert!(scheduler.transition_to_user(transitioning, 2, ANON_FLOW_WEIGHT));
        assert!(!head_has_tags(&scheduler, transitioning));
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
        assert!(head_has_tags(&scheduler, sibling));

        assert!(scheduler.transition_to_user(transitioning, 1, 10));
        assert!(!head_has_tags(&scheduler, sibling));
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
