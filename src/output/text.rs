use colored::*;
use tabled::{Table, Tabled};

pub trait TextOutput {
    fn format_text(&self, color: bool) -> String;
}

pub fn print_success(message: &str, color: bool) {
    if color {
        println!("{} {}", "✓".green().bold(), message);
    } else {
        println!("✓ {}", message);
    }
}

pub fn print_warning(message: &str, color: bool) {
    if color {
        println!("{} {}", "⚠️".yellow().bold(), message);
    } else {
        println!("⚠️ {}", message);
    }
}

pub fn print_error(message: &str, color: bool) {
    if color {
        eprintln!("{} {}", "✗".red().bold(), message);
    } else {
        eprintln!("✗ {}", message);
    }
}

pub fn print_info(message: &str, color: bool) {
    if color {
        println!("{} {}", "ℹ".blue().bold(), message);
    } else {
        println!("ℹ {}", message);
    }
}

pub fn format_table<T: Tabled>(data: Vec<T>, _color: bool) -> String {
    let table = Table::new(data);
    table.to_string()
}

// Helper functions for common formatting
pub fn format_hashrate(hashrate_mhs: f64) -> String {
    if hashrate_mhs >= 1_000_000.0 {
        format!("{:.1} TH/s", hashrate_mhs / 1_000_000.0)
    } else if hashrate_mhs >= 1_000.0 {
        format!("{:.1} GH/s", hashrate_mhs / 1_000.0)
    } else {
        format!("{:.1} MH/s", hashrate_mhs)
    }
}

pub fn format_temperature(temp_celsius: f64, color: bool) -> String {
    let temp_str = format!("{:.1}°C", temp_celsius);
    
    if !color {
        return temp_str;
    }
    
    if temp_celsius >= 80.0 {
        temp_str.red().to_string()
    } else if temp_celsius >= 70.0 {
        temp_str.yellow().to_string()
    } else {
        temp_str.green().to_string()
    }
}

pub fn format_power(power_watts: f64) -> String {
    format!("{:.1}W", power_watts)
}

pub fn format_uptime(uptime_seconds: u64) -> String {
    let days = uptime_seconds / 86400;
    let hours = (uptime_seconds % 86400) / 3600;
    let minutes = (uptime_seconds % 3600) / 60;
    
    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}

pub fn format_percentage(value: f64, total: f64, color: bool) -> String {
    let percentage = if total > 0.0 { (value / total) * 100.0 } else { 0.0 };
    let percent_str = format!("{:.1}%", percentage);
    
    if !color {
        return percent_str;
    }
    
    if percentage >= 95.0 {
        percent_str.green().to_string()
    } else if percentage >= 80.0 {
        percent_str.yellow().to_string()
    } else {
        percent_str.red().to_string()
    }
}