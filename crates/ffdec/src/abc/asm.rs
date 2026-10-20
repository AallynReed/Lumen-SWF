//! AVM2 p-code instruction assembler.

use std::collections::HashMap;

use crate::error::{Error, Result};
use super::opcodes::{Instruction, Operand, OPS};
use super::Abc;

pub fn assemble_instructions(pcode: &str, _abc: &Abc) -> Result<Vec<u8>> {
    let mut instructions = Vec::new();
    let mut labels: HashMap<String, usize> = HashMap::new();
    let mut pending_jumps: Vec<(usize, String)> = Vec::new();

    for raw_line in pcode.lines() {
        let mut line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(pos) = line.find("//") {
            line = line[..pos].trim();
            if line.is_empty() {
                continue;
            }
        }

        // Handle label prefix or standalone label: ofs0010:
        while let Some(colon) = line.find(':') {
            let candidate = line[..colon].trim();
            if candidate.starts_with("ofs") && !candidate.contains(' ') {
                labels.insert(candidate.to_string(), instructions.len());
                line = line[colon + 1..].trim();
            } else {
                break;
            }
        }
        if line.is_empty() {
            continue;
        }

        // Tokenize instruction and operands
        let (ins_name, args_str) = match line.split_once(|c: char| c.is_whitespace()) {
            Some((n, rest)) => (n.trim(), rest.trim()),
            None => (line, ""),
        };

        // Find instruction in OPS
        let op_info = OPS
            .iter()
            .find(|(_, name, _, _)| name.eq_ignore_ascii_case(ins_name))
            .ok_or_else(|| Error::Other(format!("unknown AVM2 instruction: {ins_name}")))?;

        let code = op_info.0;
        let op_name = op_info.1;
        let kinds = op_info.2;

        let mut operands = Vec::new();
        let tokens: Vec<&str> = if args_str.is_empty() {
            Vec::new()
        } else {
            args_str.split(',').map(|s| s.trim()).collect()
        };

        for (i, &kind) in kinds.iter().enumerate() {
            let token = tokens.get(i).copied().unwrap_or("0");
            match kind {
                Operand::Offset | Operand::CaseBaseoffset => {
                    let target_str = token.to_string();
                    let idx = instructions.len();
                    pending_jumps.push((idx, target_str));
                    operands.push(0);
                }
                _ => {
                    // Try parsing as integer, or hex, or ignore wrapper like Utf8("...") / Integer(...)
                    let val = parse_operand_val(token);
                    operands.push(val);
                }
            }
        }

        instructions.push(Instruction {
            code,
            name: op_name,
            offset: 0,
            len: 0,
            operands,
            kinds,
        });
    }

    // Measure instruction offsets to resolve jumps
    let mut offsets = Vec::with_capacity(instructions.len());
    let mut cur_offset = 0usize;
    for ins in &instructions {
        offsets.push(cur_offset);
        let mut tmp = crate::io::Writer::new();
        ins.encode(&mut tmp);
        cur_offset += tmp.data.len();
    }
    let total_code_size = cur_offset;

    // Resolve jumps
    for (jump_idx, label_name) in pending_jumps {
        let target_off = if let Some(&target_idx) = labels.get(&label_name) {
            if target_idx < offsets.len() {
                offsets[target_idx]
            } else {
                total_code_size
            }
        } else if let Some(hex) = label_name.strip_prefix("ofs") {
            usize::from_str_radix(hex, 16).unwrap_or(total_code_size)
        } else {
            total_code_size
        };

        let jump_off = offsets[jump_idx];
        let mut tmp = crate::io::Writer::new();
        instructions[jump_idx].encode(&mut tmp);
        let jump_len = tmp.data.len();
        let relative_offset = (target_off as i64) - ((jump_off + jump_len) as i64);

        if let Some(op) = instructions[jump_idx].operands.get_mut(0) {
            *op = relative_offset;
        }
    }

    // Encode all instructions into bytecode
    let mut out = crate::io::Writer::new();
    for ins in &instructions {
        ins.encode(&mut out);
    }
    Ok(out.data)
}

fn parse_operand_val(tok: &str) -> i64 {
    let t = tok.trim();
    if let Some(inner) = t.strip_prefix("Integer(").and_then(|s| s.strip_suffix(')')) {
        return inner.parse().unwrap_or(0);
    }
    if let Some(inner) = t.strip_prefix("UInteger(").and_then(|s| s.strip_suffix(')')) {
        return inner.parse().unwrap_or(0);
    }
    if let Some(inner) = t.strip_prefix("Unknown(").and_then(|s| s.strip_suffix(')')) {
        return inner.parse().unwrap_or(0);
    }
    if let Ok(i) = t.parse::<i64>() {
        return i;
    }
    if let Some(h) = t.strip_prefix("0x") {
        if let Ok(i) = i64::from_str_radix(h, 16) {
            return i;
        }
    }
    0
}
