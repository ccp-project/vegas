use portus::ipc::Ipc;
use portus::lang::Scope;
use portus::{CongAlg, Datapath, DatapathInfo, DatapathTrait, Flow, Report};
use std::collections::HashMap;
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct VegasConfig {
    pub alpha: u32,
    pub beta: u32,
}

impl Default for VegasConfig {
    fn default() -> Self {
        VegasConfig { alpha: 2, beta: 4 }
    }
}

impl<T: Ipc> CongAlg<T> for VegasConfig {
    type Flow = Vegas<T>;

    fn name() -> &'static str {
        "vegas"
    }

    fn datapath_programs(&self) -> HashMap<&'static str, String> {
        let mut h = HashMap::default();
        h.insert(
            "vegas",
            "
                (def (Report
                        (volatile acked 0)
                        (volatile rtt 0)
                ))
                (when true
                    (:= Report.acked (+ Report.acked Ack.bytes_acked))
                    (:= Report.rtt Flow.rtt_sample_us)
                    (fallthrough)
                )
                (when (> Micros Flow.rtt_sample_us)
                    (report)
                    (:= Micros 0)
                )
            "
            .to_owned(),
        );
        h
    }

    fn new_flow(&self, mut control: Datapath<T>, info: DatapathInfo) -> Self::Flow {
        let sc = control
            .set_program("vegas", None)
            .expect("set vegas datapath program");
        Vegas {
            control_channel: control,
            alpha: self.alpha,
            beta: self.beta,
            sc,
            mss: info.mss,
            cwnd: info.init_cwnd,
            min_rtt: f32::MAX,
        }
    }
}

#[derive(Clone)]
pub struct Vegas<T: Ipc> {
    control_channel: Datapath<T>,
    alpha: u32,
    beta: u32,
    sc: Scope,
    mss: u32,
    cwnd: u32,
    min_rtt: f32,
}

impl<T: Ipc> Vegas<T> {
    fn update_cwnd(&self) {
        if let Err(err) = self
            .control_channel
            .update_field(&self.sc, &[("Cwnd", self.cwnd)])
        {
            warn!(?err, "Cwnd update error");
        }
    }

    fn get_fields(&mut self, m: &Report) -> Option<(u32, u32)> {
        let acked = m
            .get_field(&String::from("Report.acked"), &self.sc)
            .expect("expected acked field in returned measurement") as u32;
        let rtt = m
            .get_field(&String::from("Report.rtt"), &self.sc)
            .expect("expected rtt field in returned measurement") as u32;
        Some((acked, rtt))
    }
}

impl<T: Ipc> Flow for Vegas<T> {
    fn on_report(&mut self, _sock_id: u32, r: Report) {
        let fields = self.get_fields(&r);
        if fields.is_none() {
            return;
        }
        let (bytes_acked, rtt_us) = fields.unwrap();
        let curr_rtt = (rtt_us as f32) / 1_000_000.0;

        if curr_rtt < self.min_rtt {
            self.min_rtt = curr_rtt;
        }

        let in_queue =
            ((self.cwnd as f32) * (curr_rtt - self.min_rtt) / (curr_rtt * self.mss as f32)) as u32;
        if in_queue <= self.alpha {
            self.cwnd += self.mss;
        } else if in_queue >= self.beta {
            self.cwnd -= self.mss;
        }

        self.update_cwnd();
        info!(?bytes_acked, ?curr_rtt, ?in_queue, ?self.cwnd, ?self.min_rtt, "got report");
    }
}
