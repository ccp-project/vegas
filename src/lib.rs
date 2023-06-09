#[macro_use]
extern crate slog;
extern crate time;
extern crate portus;

use portus::{CongAlg, Config, Datapath, DatapathInfo, DatapathTrait, Report};
use portus::ipc::Ipc;
use portus::lang::Scope;

const INIT_CWND_PKTS: u32 = 10;

pub struct Vegas<T: Ipc> {
    control_channel: Datapath<T>,
    logger: Option<slog::Logger>,
    sc: Scope,
    mss: u32,
    cwnd: u32,
    min_rtt: f32,
    alpha: u32,
    beta: u32,
}
#[derive(Clone)]
pub struct VegasConfig {
    pub alpha: u32,
    pub beta:  u32,
}

impl Default for VegasConfig {
    fn default() -> Self {
        VegasConfig {
            alpha: 2,
            beta: 4,
        }
    }
}

impl<T: Ipc> Vegas<T> {
    fn update_cwnd(&self) {
        if let Err(e) = self.control_channel
            .update_field(&self.sc, &[("Cwnd", self.cwnd)]) 
        {
            self.logger.as_ref().map(|log| {
                warn!(log, "Cwnd update error";
                      "err" => ?e,
                );
            });
        }
    }

    fn install_program(&self) -> Scope {
        self.control_channel.install(
            b"
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
            ", None
        ).unwrap()
    }

    fn get_fields(&mut self, m: &Report) -> Option<(u32, u32)> {
        let acked = m.get_field(&String::from("Report.acked"), &self.sc).expect(
            "expected acked field in returned measurement",
        ) as u32;
        let rtt = m.get_field(&String::from("Report.rtt"), &self.sc).expect(
            "expected rtt field in returned measurement",
        ) as u32;
        Some((acked, rtt))
    }

}

impl<T: Ipc> CongAlg<T> for Vegas<T> {
    type Config = VegasConfig;

    fn name() -> String {
        String::from("vegas")
    }
    
    fn create(control: Datapath<T>, cfg: Config<T, Vegas<T>>, info: DatapathInfo) -> Self {
        let mut s = Self {
            control_channel: control,
            logger: cfg.logger,
            sc: Scope::new(),
            mss: info.mss,
            cwnd: INIT_CWND_PKTS * info.mss,
            min_rtt: 0.0,
            alpha: cfg.config.alpha,
            beta: cfg.config.beta,
        };
        
        s.sc = s.install_program();
        s
    }

    fn on_report(&mut self, _sock_id: u32, r: Report) {
        let fields = self.get_fields(&r);
        if fields.is_none() {
            return;
        }
        let (bytes_acked, rtt_us) = fields.unwrap();
        let curr_rtt = (rtt_us as f32) / 1_000_000.0;
        
        if self.min_rtt <= 0.0 || curr_rtt < self.min_rtt {
            self.min_rtt = curr_rtt;
        }

        let in_queue = ((self.cwnd as f32) * (curr_rtt - self.min_rtt) / (curr_rtt * self.mss as f32)) as u32;
        if in_queue <= self.alpha {
            self.cwnd += self.mss;
        } else if in_queue >= self.beta {
            self.cwnd -= self.mss;
        }

        self.update_cwnd();

        self.logger.as_ref().map(|log| {
            info!(log, "got report";
                  "bytesAcked" => bytes_acked,
                  "curr_rtt" => curr_rtt,
                  "min_rtt" => self.min_rtt,
                  "in_queue" => in_queue,
                  "cwnd" => self.cwnd,
            );   
        });
    }
}
