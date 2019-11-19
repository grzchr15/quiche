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

// Reno Congestion Control
#[derive(Debug)]
pub struct Reno {
    congestion_window: usize,

    bytes_in_flight: usize,

    congestion_recovery_start_time: Option<Instant>,

    ssthresh: usize,
    /* TODO: ECN is not implemented.
     * ecn_ce_counters: [usize; packet::EPOCH_COUNT] */
}

impl cc::CongestionControl for Reno {
    fn new() -> Self
    where
        Self: Sized,
    {
        Reno {
            congestion_window: cc::INITIAL_WINDOW,

            bytes_in_flight: 0,

            congestion_recovery_start_time: None,

            ssthresh: std::usize::MAX,
            /* TODO: ECN is not implemented.
             * ecn_ce_counters: [0; packet::EPOCH_COUNT], */
        }
    }

    fn cwnd(&self) -> usize {
        self.congestion_window
    }

    fn set_cwnd(&mut self, cwnd: usize) {
        self.congestion_window = cwnd;
    }

    fn ssthresh(&self) -> usize {
        self.ssthresh
    }

    fn bytes_in_flight(&self) -> usize {
        self.bytes_in_flight
    }

    fn set_bytes_in_flight(&mut self, bytes_in_flight: usize) {
        self.bytes_in_flight = bytes_in_flight;
    }

    fn congestion_recovery_start_time(&self) -> Option<Instant> {
        self.congestion_recovery_start_time
    }

    // Congestion Control hooks
    fn on_packet_sent_cc(&mut self, bytes_sent: usize, trace_id: &str) {
        self.bytes_in_flight += bytes_sent;

        trace!(
            "{} CC_DEBUG OnPacketSentCC() bytes_in_flight={}",
            trace_id,
            self.bytes_in_flight
        );
    }

    fn on_packet_acked_cc(&mut self, packet: &Sent, trace_id: &str) {
        self.bytes_in_flight -= packet.size;

        if self.in_congestion_recovery(packet.time) {
            return;
        }

        if self.is_app_limited() {
            return;
        }

        if self.congestion_window < self.ssthresh {
            // Slow start.
            self.congestion_window += packet.size;
            trace!(
                "{} CC_DEBUG OnPacketAckedCC() SLOW_START cwnd={} ssthresh={}",
                trace_id,
                self.congestion_window,
                self.ssthresh
            );
        } else {
            // Congestion avoidance.
            self.congestion_window +=
                (cc::MAX_DATAGRAM_SIZE * packet.size) / self.congestion_window;
            trace!(
                "{} CC_DEBUG OnPacketAckedCC() CONG_AVOIDANCE cwnd={} ssthresh={}",
                trace_id,
                self.congestion_window,
                self.ssthresh
            );
        }
    }

    fn congestion_event(
        &mut self, time_sent: Instant, now: Instant, trace_id: &str,
    ) {
        // Start a new congestion event if packet was sent after the
        // start of the previous congestion recovery period.
        if !self.in_congestion_recovery(time_sent) {
            self.congestion_recovery_start_time = Some(now);

            self.congestion_window = (self.congestion_window as f64 *
                cc::LOSS_REDUCTION_FACTOR)
                as usize;
            self.congestion_window =
                std::cmp::max(self.congestion_window, cc::MINIMUM_WINDOW);
            self.ssthresh = self.congestion_window;

            trace!(
                "{} CC_DEBUG CongestionEvent() cwnd={} ssthresh={}",
                trace_id,
                self.congestion_window,
                self.ssthresh
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACE_ID: &str = "test_id";

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn reno_init() {
        let cc = cc::new_congestion_control(cc::Algorithm::Reno);

        assert!(cc.cwnd() > 0);
        assert_eq!(cc.bytes_in_flight(), 0);
        assert_eq!(cc.ssthresh(), std::usize::MAX);
    }

    #[test]
    fn reno_send() {
        init();

        let mut cc = cc::new_congestion_control(cc::Algorithm::Reno);

        cc.on_packet_sent_cc(1000, TRACE_ID);

        assert_eq!(cc.bytes_in_flight(), 1000);
    }

    #[test]
    fn reno_slow_start() {
        init();

        let mut cc = cc::new_congestion_control(cc::Algorithm::Reno);

        let p = Sent {
            pkt_num: 0,
            frames: vec![],
            time: std::time::Instant::now(),
            size: 1111,
            ack_eliciting: true,
            in_flight: true,
        };

        cc.on_packet_sent_cc(p.size, TRACE_ID);

        let cwnd_prev = cc.cwnd();

        cc.on_packet_acked_cc(&p, TRACE_ID);

        // Check if cwnd increased by packet size (slow start)
        assert_eq!(cc.cwnd(), cwnd_prev + p.size);
    }

    #[test]
    fn reno_congestion_event() {
        init();

        let mut cc = cc::new_congestion_control(cc::Algorithm::Reno);

        cc.congestion_event(
            std::time::Instant::now(),
            std::time::Instant::now(),
            TRACE_ID,
        );

        // In Reno, after congestion event, cwnd = ssthresh
        assert_eq!(cc.cwnd(), cc.ssthresh());
    }

    #[test]
    fn reno_congestion_avoidance() {
        init();

        let mut cc = cc::new_congestion_control(cc::Algorithm::Reno);

        // Send 20K bytes
        cc.on_packet_sent_cc(20000, TRACE_ID);

        cc.congestion_event(
            std::time::Instant::now(),
            std::time::Instant::now(),
            TRACE_ID,
        );

        // In Reno, after congestion event, cwnd = ssthresh
        assert_eq!(cc.cwnd(), cc.ssthresh());

        let p = Sent {
            pkt_num: 0,
            frames: vec![],
            time: std::time::Instant::now(),
            size: 5000,
            ack_eliciting: true,
            in_flight: true,
        };

        let cwnd_prev = cc.cwnd();

        // Ack 5000 bytes
        cc.on_packet_acked_cc(&p, TRACE_ID);

        // Check if cwnd increase is smaller than a packet size (congestion
        // avoidance)
        assert!(cc.cwnd() < cwnd_prev + 1111);
    }
}
