//! 物理护栏校准（对应 `human_guard.cpp`）。
//!
//! 从固件监控帧里按 X/Y 两轴独立检测方向反转，记录相邻反转的最小毫秒间隔，
//! 结果经 `human_guard` 事件上报（min / p5 / median / suggest / recent）。

use serde_json::Value;

use ysc_core::util::now_ms;

use crate::app::{App, AxisStat, GuardState, HG_MAX_MS, HG_MIN_MS, HG_RECENT_MAX};

const HG_HYST: i32 = 3;
const HG_GAP_RESET_MS: u32 = 50;
const HG_MAX_DURATION: i32 = 15000;

impl App {
    pub fn guard_on_frame(&self, data: &[u8]) {
        {
            let g = self.guard.lock().unwrap();
            if !g.active {
                return;
            }
        }
        if data.is_empty() || data.len() >= 1024 {
            return;
        }
        let Ok(text) = std::str::from_utf8(data) else {
            return;
        };
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            return;
        };
        // 只处理鼠标监控帧：message 为 null / 缺失 / "mouse"。
        if let Some(m) = v.get("message") {
            match m {
                Value::Null => {}
                Value::String(s) if s == "mouse" => {}
                _ => return,
            }
        }
        let Some(inner) = v.get("data").and_then(|d| d.as_str()) else {
            return;
        };
        let Ok(iv) = serde_json::from_str::<Value>(inner) else {
            return;
        };
        let (Some(x), Some(y)) = (
            iv.get("x").and_then(|n| n.as_i64()),
            iv.get("y").and_then(|n| n.as_i64()),
        ) else {
            return;
        };
        let t = iv.get("t").and_then(|n| n.as_u64());

        let mut g = self.guard.lock().unwrap();
        if !g.active {
            return;
        }
        let t = match t {
            Some(t) => {
                g.saw_t = true;
                t as u32
            }
            None => {
                g.saw_no_t = true;
                now_ms() as u32
            }
        };
        g.frames += 1;
        feed_axis(&mut g.x, x as i32, t);
        feed_axis(&mut g.y, y as i32, t);
    }

    pub fn guard_begin(&self, axis: &str, duration_ms: i32) -> bool {
        let ax = if axis.starts_with('y') || axis.starts_with('Y') {
            'y'
        } else {
            'x'
        };
        let duration = duration_ms.clamp(1000, HG_MAX_DURATION);
        let start = now_ms();
        {
            let mut g = self.guard.lock().unwrap();
            *g = GuardState {
                active: true,
                stop_requested: false,
                x: AxisStat::default(),
                y: AxisStat::default(),
                axis: ax,
                duration,
                start_ms: start,
                last_progress_ms: start,
                session: start as u32,
                frames: 0,
                saw_t: false,
                saw_no_t: false,
                last_result: None,
            };
        }
        let body = self.build_guard_body("running", "running");
        self.send_event("human_guard", Some(&body));
        true
    }

    pub fn guard_stop(&self) {
        let mut g = self.guard.lock().unwrap();
        if g.active {
            g.stop_requested = true;
        }
    }

    pub fn guard_abort(&self, reason: &str) {
        let was_active = {
            let mut g = self.guard.lock().unwrap();
            if g.active {
                g.active = false;
                true
            } else {
                false
            }
        };
        if was_active {
            self.send_guard_final(reason);
        }
    }

    pub fn guard_status(&self) {
        let (active, last) = {
            let g = self.guard.lock().unwrap();
            (g.active, g.last_result.clone())
        };
        if active {
            let body = self.build_guard_body("running", "running");
            self.send_event("human_guard", Some(&body));
        } else if let Some(last) = last {
            self.send_event("human_guard", Some(&last));
        } else {
            self.send_event("human_guard", Some("\"phase\":\"idle\""));
        }
    }

    /// 50ms 心跳：超时 / 提前停止 / 100ms 进度。
    pub fn guard_tick(&self) {
        let (finish, reason, progress) = {
            let g = self.guard.lock().unwrap();
            if !g.active {
                (false, "running", false)
            } else {
                let now = now_ms();
                if g.stop_requested {
                    (true, "stopped", false)
                } else if now.saturating_sub(g.start_ms) >= g.duration as u64 {
                    (true, "timeout", false)
                } else if now.saturating_sub(g.last_progress_ms) >= 100 {
                    (false, "running", true)
                } else {
                    (false, "running", false)
                }
            }
        };
        if progress {
            {
                let mut g = self.guard.lock().unwrap();
                g.last_progress_ms = now_ms();
            }
            let body = self.build_guard_body("running", "running");
            self.send_event("human_guard", Some(&body));
        }
        if finish {
            {
                let mut g = self.guard.lock().unwrap();
                g.active = false;
            }
            self.send_guard_final(reason);
        }
    }

    pub fn guard_shutdown(&self) {
        self.guard_abort("shutdown");
    }

    fn send_guard_final(&self, reason: &str) {
        let is_error = matches!(reason, "device_lost" | "shutdown" | "restarted");
        let phase = if is_error { "error" } else { "done" };
        let body = self.build_guard_body(phase, reason);
        let mut g = self.guard.lock().unwrap();
        g.last_result = Some(body.clone());
        drop(g);
        self.send_event("human_guard", Some(&body));
    }

    fn build_guard_body(&self, phase: &str, reason: &str) -> String {
        let (x, y, axis, duration, frames, session, start, timebase) = {
            let g = self.guard.lock().unwrap();
            (
                g.x.clone(),
                g.y.clone(),
                g.axis,
                g.duration,
                g.frames,
                g.session,
                g.start_ms,
                if g.saw_t && g.saw_no_t {
                    "mixed"
                } else if g.saw_t {
                    "device"
                } else if g.saw_no_t {
                    "host"
                } else {
                    "none"
                },
            )
        };
        let elapsed = (now_ms().saturating_sub(start)).min(u32::MAX as u64);
        let xj = build_axis_json(&x);
        let yj = build_axis_json(&y);
        format!(
            "\"phase\":\"{}\",\"session\":{},\"axis\":\"{}\",\"duration\":{},\"elapsed\":{},\"frames\":{},\"timebase\":\"{}\",\"reason\":\"{}\",\"dirX\":{},\"dirY\":{},\"x\":{{{}}},\"y\":{{{}}}",
            phase,
            session,
            axis,
            duration,
            elapsed,
            frames,
            timebase,
            reason,
            x.dir,
            y.dir,
            xj,
            yj
        )
    }
}

fn feed_axis(st: &mut AxisStat, delta: i32, t: u32) {
    if st.last_t != 0 {
        let gap = t.wrapping_sub(st.last_t);
        if gap > HG_GAP_RESET_MS {
            st.dir = 0;
            st.opp_acc = 0;
        }
    }
    st.last_t = t;
    if delta == 0 {
        return;
    }
    let s = if delta > 0 { 1 } else { -1 };
    if st.dir == 0 {
        st.dir = s;
        st.rev_t = t;
        st.opp_acc = 0;
        return;
    }
    if s == st.dir {
        st.opp_acc = 0;
        return;
    }
    st.opp_acc += delta;
    if st.opp_acc >= HG_HYST || st.opp_acc <= -HG_HYST {
        let dt = t.wrapping_sub(st.rev_t);
        if dt >= HG_MIN_MS && dt <= HG_MAX_MS {
            st.hist[dt as usize] += 1;
            st.count += 1;
            st.last_ms = dt;
            if st.min_ms == 0 || dt < st.min_ms {
                st.min_ms = dt;
            }
            st.recent[(st.recent_count % HG_RECENT_MAX) as usize] = dt;
            st.recent_count += 1;
        }
        st.dir = s;
        st.rev_t = t;
        st.opp_acc = 0;
    }
}

fn compute_stats(st: &AxisStat) -> (u32, u32, bool) {
    let mut p5 = 0;
    let mut median = 0;
    let reliable = st.count >= 5;
    if st.count == 0 {
        return (p5, median, reliable);
    }
    let need5 = ((st.count * 5 + 99) / 100).max(1);
    let need50 = (st.count + 1) / 2;
    let mut cum = 0u32;
    let mut got5 = false;
    for d in HG_MIN_MS..=HG_MAX_MS {
        let c = st.hist[d as usize];
        if c == 0 {
            continue;
        }
        cum += c;
        if !got5 && cum >= need5 {
            p5 = d;
            got5 = true;
        }
        if cum >= need50 {
            median = d;
            break;
        }
    }
    (p5, median, reliable)
}

fn build_axis_json(st: &AxisStat) -> String {
    let (p5, median, reliable) = compute_stats(st);
    let suggest = if reliable { st.min_ms } else { 0 };
    let mut s = format!(
        "\"count\":{},\"min\":{},\"p5\":{},\"median\":{},\"last\":{},\"suggest\":{},\"reliable\":{},\"recent\":[",
        st.count, st.min_ms, p5, median, st.last_ms, suggest, reliable
    );
    let total = st.recent_count;
    let cnt = total.min(HG_RECENT_MAX);
    let start = if total > HG_RECENT_MAX {
        total % HG_RECENT_MAX
    } else {
        0
    };
    for i in 0..cnt {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&st.recent[((start + i) % HG_RECENT_MAX) as usize].to_string());
    }
    s.push(']');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;

    #[test]
    fn reversal_intervals_are_recorded() {
        let app = App::test_app();
        app.guard_begin("x", 5000);
        // 0 → +5 → -5 → +5 → -5，间隔 10/20/30ms
        app.guard_on_frame(
            br#"{"code":200,"message":null,"data":"{\"b\":0,\"x\":0,\"y\":0,\"t\":100}"}"#,
        );
        for (t, x) in [(110u32, 5i32), (120, -5), (140, 5), (170, -5)] {
            let frame = format!(
                "{{\"code\":200,\"message\":null,\"data\":\"{{\\\"b\\\":0,\\\"x\\\":{},\\\"y\\\":0,\\\"t\\\":{}}}\"}}",
                x, t
            );
            app.guard_on_frame(frame.as_bytes());
        }
        let g = app.guard.lock().unwrap();
        assert!(g.x.count >= 3, "count={}", g.x.count);
        assert_eq!(g.x.min_ms, 10);
        assert!(g.x.hist[10] >= 1 && g.x.hist[20] >= 1 && g.x.hist[30] >= 1);
        drop(g);
        app.guard_abort("stopped");
        assert!(!app.guard.lock().unwrap().active);
    }

    #[test]
    fn stats_percentiles() {
        let mut st = AxisStat::default();
        for d in [10u32, 10, 20, 30, 40, 50, 60, 70, 80, 90] {
            st.hist[d as usize] += 1;
            st.count += 1;
        }
        let (p5, median, reliable) = compute_stats(&st);
        assert!(reliable);
        assert_eq!(p5, 10);
        assert_eq!(median, 40); // need50=(10+1)/2=5，累加到 40 时已达 5
    }
}
