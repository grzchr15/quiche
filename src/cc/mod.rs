// Copyright (C) 2019, Cloudflare, Inc.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
//     * Redistributions of source code must retain the above copyright notice,
//       this list of conditions and the following disclaimer.
//
//     * Redistributions in binary form must reproduce the above copyright
//       notice, this list of conditions and the following disclaimer in the
//       documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS
// IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO,
// THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
// CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL,
// EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR
// PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF
// LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING
// NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
use std::time::Instant;

use crate::cc;
use crate::recovery::Sent;

// Congestion Control constants
pub const INITIAL_WINDOW_PACKETS: usize = 10;

pub const INITIAL_WINDOW: usize = INITIAL_WINDOW_PACKETS * MAX_DATAGRAM_SIZE;

pub const MINIMUM_WINDOW: usize = 2 * MAX_DATAGRAM_SIZE;

pub const MAX_DATAGRAM_SIZE: usize = 1452;

pub const LOSS_REDUCTION_FACTOR: f64 = 0.5;

// Available CC algorithms.
// This is defined in include/quiche.h as well.
// When you add a new CC, update here and include/quiche.h both.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Algorithm {
    Reno = 0, // "reno"
}

impl std::fmt::Debug for dyn CongestionControl {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "cwnd={} ssthresh={} bytes_in_flight={}",
            self.cwnd(),
            self.ssthresh(),
            self.bytes_in_flight()
        )
    }
}

// Congestion Control Trait
pub trait CongestionControl {
    fn new() -> Self
    where
        Self: Sized;

    // Access to internal variables
    fn cwnd(&self) -> usize;

    fn set_cwnd(&mut self, cwnd: usize);

    fn ssthresh(&self) -> usize;

    fn bytes_in_flight(&self) -> usize;

    fn set_bytes_in_flight(&mut self, bytes_in_flight: usize);

    fn congestion_recovery_start_time(&self) -> Option<Instant>;

    // Reset to minimum window.
    fn collapse_cwnd(&mut self) {
        self.set_cwnd(cc::MINIMUM_WINDOW);
    }

    // Congestion Control hooks defined in QUIC recovery draft.

    // OnPacketSentCC(bytes_sent)
    fn on_packet_sent_cc(&mut self, bytes_sent: usize, trace_id: &str);

    // InCongestionRecovery(sent_time)
    fn in_congestion_recovery(&self, sent_time: Instant) -> bool {
        match self.congestion_recovery_start_time() {
            Some(congestion_recovery_start_time) =>
                sent_time <= congestion_recovery_start_time,

            None => false,
        }
    }

    // IsAppLimited()
    // TODO: need to implement
    fn is_app_limited(&self) -> bool {
        false
    }

    // OnPacketAckedCC(packet)
    fn on_packet_acked_cc(&mut self, packet: &Sent, trace_id: &str);

    // CongestionEvent(time_sent)
    // now is passed as well not to look up current time again.
    fn congestion_event(
        &mut self, time_sent: Instant, now: Instant, trace_id: &str,
    );
}

// Returns a congestion control module, specified as `name`.
pub fn new_congestion_control(algo: Algorithm) -> Box<dyn CongestionControl> {
    trace!("CC_DEBUG get_congestion_control: {:?}", algo);
    match algo {
        Algorithm::Reno => Box::new(cc::reno::Reno::new()),
    }
}

// Return Algorithm enum from the string. This is mainly for command line
// options support.
pub fn lookup_cc_algorithm(name: &str) -> Algorithm {
    match name {
        "reno" => Algorithm::Reno,
        _ => Algorithm::Reno,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cc() {
        let cc = new_congestion_control(Algorithm::Reno);

        assert!(cc.cwnd() > 0);
        assert_eq!(cc.bytes_in_flight(), 0);
        assert_eq!(cc.ssthresh(), std::usize::MAX);
    }

    #[test]
    fn lookup_cc_algo() {
        let algo = lookup_cc_algorithm("reno");

        assert_eq!(algo, Algorithm::Reno);

        let algo = lookup_cc_algorithm("???");

        assert_eq!(algo, Algorithm::Reno);
    }
}

// List of CC modules.
mod reno;
