use tokio;
use tokio::time::sleep;
use std::time::Duration;
use tokio::io::{BufReader, AsyncBufReadExt};
use tokio::fs::File;
use chrono::{Local};
use swayipc::{Connection};
#[macro_use] extern crate scan_fmt;

const CPU_TEMP_PATH: &str = "/sys/class/thermal/thermal_zone0/temp";
const AC_ONLINE_PATH: &str = "/sys/class/power_supply/AC/online";
const BAT_CAP_PATH: &str = "/sys/class/power_supply/BAT0/capacity"; 
const BAT_STAT_PATH: &str = "/sys/class/power_supply/BAT0/status";  
const BAT1_CAP_PATH: &str = "/sys/class/power_supply/BAT1/capacity";
const BAT1_STAT_PATH: &str = "/sys/class/power_supply/BAT1/status";
const CPU_STAT_PATH: &str = "/proc/stat";
const MEMINFO_PATH: &str = "/proc/meminfo";

enum PrintColor {
    White,
    Black,
    Red,
    Red2,
    Red3,
    Green,
    Blue,
    Orange,
    Yellow
}

fn p_full(text: &str, color: PrintColor) -> String {
    let mut p = String::with_capacity(100);
    let cl:&str;
    match color {
        PrintColor::White => {
            cl = "#FFFFFF";
        },
        PrintColor::Black => {
            cl = "#000000";
        },
        PrintColor::Red => {
            cl = "#ed3d40";
        },
        PrintColor::Green => {
            cl = "#00FF00";
        },
        PrintColor::Blue => {
            cl = "#66a1f9";
        },
        PrintColor::Orange => {
            cl = "#f4e71a";
        },
        PrintColor::Yellow => {
            cl = "#ffff00";
        },
        PrintColor::Red2 => {
            cl = "#E87605";
        },
        PrintColor::Red3 => {
            cl = "#FF2301";
        }
    }
    p.push_str("{\"full_text\":\"");
    p.push_str(text);
    p.push_str("\",\"color\":\"");
    p.push_str(cl);
    p.push_str("\"}");
    
    p
}

async fn read_line(path: &str) -> Option<String> {
    match File::open(path).await {
        Ok(v) => {
            let mut line = String::with_capacity(255);
            let mut reader = BufReader::new(v);
            match reader.read_line(&mut line).await {
                Ok(_reader_len) => {
                    // do not count last symbol. It it is new line symbol
                    line.pop();
                    Some(line)
                },
                Err(_e) => None
            }
        },
        Err(_e) => None
    }
}

async fn p_cpu_temp() -> String {
    let temp:Option<u32> = match read_line(CPU_TEMP_PATH).await {
        Some(line) => {            
            match line.parse::<u32>() {
                Ok(parsed_temp) => Some(parsed_temp / 1000),
                Err(_e) => None
            }
        },
        None => None
    };
    let color:PrintColor = match temp {
        Some(x) if x > 90 => PrintColor::Red3,
        Some(x) if x > 80 => PrintColor::Red,
        Some(x) if x > 70 => PrintColor::Orange,
        Some(x) if x > 65 => PrintColor::Yellow,
        _ => PrintColor::Green,
    };
    match temp {
        None => p_full("CPU NA°C", PrintColor::Red2),
        Some(temp) => p_full(&format!("CPU {}°C", temp), color),
    }
}

async fn p_cpu_usage(last: (u64, u64, u64, u64)) -> ((u64, u64, u64, u64), String) {
    let (last_total_user, last_total_user_low, last_total_sys, last_total_idle) = last;
    let mut cur: (u64, u64, u64, u64) = (0, 0, 0, 0);
    let total:u64;
    let percent:Option<u32> = match read_line(CPU_STAT_PATH).await {
        Some(line) => {            
            match scan_fmt!(&line, "cpu {} {} {} {}", u64, u64, u64, u64) {
                Ok((total_user, total_user_low, total_sys, total_idle)) => {
                    cur = (total_user, total_user_low, total_sys, total_idle);
                    if total_user < last_total_user || total_user_low < last_total_user_low ||
                        total_sys < last_total_sys || total_idle < last_total_idle {
                        //Overflow detection. Just skip this value.
                        None
                    } else {
                        total = (total_user - last_total_user) + (total_user_low - last_total_user_low) + (total_sys - last_total_sys);
                            Some(( ( ( total as f32) /
                                      ((total + total_idle - last_total_idle) as f32))
                                       * 100.0)
                                 .round() as u32)
                    }
                },
                Err(_e) => None
            }
        },
        None => None
    };
    let out_string = match percent {
        None => p_full("CPU NA", PrintColor::White),
        Some(percent) => p_full(&format!("CPU {}", percent), PrintColor::White),
    };
    (cur, out_string)
}

async fn p_mem_usage() -> String {
    let percent:Option<(u32, u32)> = match File::open(MEMINFO_PATH).await {
        Ok(v) => {
            let mut line = String::with_capacity(255);
            let mut reader = BufReader::new(v);
            let memtotal = match reader.read_line(&mut line).await {
                Ok(_reader_len) => {
                    // do not count last symbol. It it is new line symbol
                    line.pop();
                    match scan_fmt!(&line, "MemTotal:{*[ ]}{} kB", u32) {
                        Ok(s) => Some(s),
                        Err(_e) => None
                    }
                },
                Err(_e) => None
            };
            // Skip MemFree
            match reader.read_line(&mut line).await {
                Ok(_v) => Some(0),
                Err(_e) => None
            };
            line.clear();
            let memavailable = match reader.read_line(&mut line).await {
                Ok(_reader_len) => {
                    // do not count last symbol. It it is new line symbol
                    line.pop();
                    match scan_fmt!(&line, "MemAvailable:{*[ ]}{} kB", u32) {
                        Ok(s) => Some(s),
                        Err(_e) => None
                    }
                },
                Err(_e) => None
            };
            // Skip Buffers
            match reader.read_line(&mut line).await {
                Ok(_v) => Some(0),
                Err(_e) => None
            };
            // Skip Cached
            match reader.read_line(&mut line).await {
                Ok(_v) => Some(0),
                Err(_e) => None
            };
            line.clear();
            let swap = match reader.read_line(&mut line).await {
                Ok(_reader_len) => {
                    // do not count last symbol. It it is new line symbol
                    line.pop();
                    match scan_fmt!(&line, "SwapCached:{*[ ]}{} kB", u32) {
                        Ok(s) => Some(s),
                        Err(_e) => None
                    }
                },
                Err(_e) => None
            };
            match swap {
                Some(sw) => match memtotal {
                    Some(tot) => match memavailable {
                        Some(ava) => Some((sw, 100 - (ava * 100 / tot))),
                        _ => None
                    },
                    _ => None
                },
                _ => None
            }
        },
        Err(_e) => None
    };
    
    match percent {
        None => p_full("RAM NA", PrintColor::White),
        Some(percent) => {
            let (sw, percent) = percent;
            if sw == 0 {
                p_full(&format!("RAM {}", percent), PrintColor::White)
            } else {
                p_full(&format!("RAM {}% SWAP {}Mb", percent, sw / 1024), PrintColor::Red)
            }
        }
    }
}

async fn p_bat(cap_path: &str, stat_path: &str, prefix: &str) -> String {
    let cap:Option<u32> = match read_line(cap_path).await {
        Some(line) => {            
            match line.parse::<u32>() {
                Ok(parsed_temp) => Some(parsed_temp),
                Err(_e) => None
            }
        },
        None => None
    };
    let mut symbol = " ";
    let discharging = match read_line(stat_path).await {
        Some(line) => {
            if line.starts_with("Dis") {
                symbol = "↓";
                true
            } else if line.starts_with("Unk") {
                true
            } else  if line.starts_with("Cha") {
                symbol = "↑";
                false
            } else {
                symbol = "?";
                false
            }
        },
        None => true
    };
    let color:PrintColor = match discharging {
        false => PrintColor::Green,
        true => match cap {
            Some(x) if x > 50 => PrintColor::White,
            Some(x) if x > 30 => PrintColor::Orange,
            Some(x) if x > 20 => PrintColor::Yellow,
            _ => PrintColor::Red
        }
    };
    match cap {
        None => p_full(&format!("{} NA", prefix), PrintColor::Red2),
        Some(cap) => p_full(&format!("{}{}{}", prefix, symbol, cap), color),
    }
}

async fn p_ac_online() -> String {
    match read_line(AC_ONLINE_PATH).await {
        Some(line) => if line == "1" {
            p_full("⚡", PrintColor::Green)
        } else {
            p_full(" ", PrintColor::Red2)
        },
        None => p_full("AC NA", PrintColor::Red2)
    }
}

fn p_time() -> String {
    let now = Local::now();
    p_full(&format!("{}", now.format("%H:%M:%S")), PrintColor::White)
}
fn p_date() -> String {
    let now = Local::now();
    p_full(&format!("{}", now.format("%a %d:%b:%Y")), PrintColor::White)
}
async fn p_lang(conn: &mut Option<Connection>) -> String {
    let mut lang = "NaN".to_string();
    match conn {
        Some(c) => match c.get_inputs().await {
            Ok(v) => {
                for i in v {
                    match i.xkb_active_layout_name {
                        Some(x) => { lang = x; break; },
                        None => continue
                    };
                }
            },
            Err(_e) => {}
        },
        None => {}
    };

    if lang == "Russian" {
        p_full(&lang[0..3], PrintColor::Blue)
    } else {
        p_full(&lang[0..3], PrintColor::Red)
    }
}

#[tokio::main]
async fn main() {
    print!("{{ \"version\": 1 }}\n[");

    let mut swayipc_conn = match Connection::new().await {
        Ok(c) => Some(c),
        Err(_e) => None
    };
    let mut last_cpu_usage: (u64, u64, u64, u64) = (0, 0, 0, 0);
    let mut string = String::new();
    loop {
        sleep(Duration::from_millis(500)).await;
        {
            // CPU temp
            string.push_str(&p_cpu_temp().await);
            string.push(',');
        }
        {
            // CPU usage
            let (last_cpu, cpu_usage_string) = p_cpu_usage(last_cpu_usage).await;
            last_cpu_usage = last_cpu;
            string.push_str(&cpu_usage_string);
            string.push(',');
        }
        {
            // RAM usage
            string.push_str(&p_mem_usage().await);
            string.push(',');
        }
        {
            // External battery
            string.push_str(&p_bat(BAT1_CAP_PATH,
                                   BAT1_STAT_PATH, "E").await);
            string.push(',');
        }
        {
            // Internal battery
            string.push_str(&p_bat(BAT_CAP_PATH,
                                   BAT_STAT_PATH, "I").await);
            string.push(',');
        }
        {
            // Is AC online
            string.push_str(&p_ac_online().await);
            string.push(',');
        }
        {
            // Layout language
            string.push_str(&p_lang(&mut swayipc_conn).await);
            string.push(',');
        }
        {
            // Date
            string.push_str(&p_date());
            string.push(',');
        }
        {
            // Time
            string.push_str(&p_time());
        }
        
        println!("[{}],", string);
        string.clear();
    }
}
