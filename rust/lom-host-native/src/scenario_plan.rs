//! Candidate ordering/group membership only. Eligibility, random draws and
//! owned state changes stay in ScenarioEngine in their original order.
use std::{collections::HashMap, mem::size_of, slice};

#[repr(C)]
pub struct Candidate {
    pub priority: i32,
    pub declaration: i32,
    pub pool: i32,
}

/// Fill sorted candidate indices plus each candidate's group offset/count and
/// grouped members. Equal pool integers mean the same original ordinal pool;
/// nonnegative pools are grouped; each negative value denotes a singleton.
/// # Safety
/// Input and four writable output arrays address count items, are aligned,
/// disjoint and remain exclusively borrowed for this synchronous call.
#[no_mangle]
pub unsafe extern "C" fn lom_scenario_plan_v1(
    input: *const Candidate,
    count: i32,
    order: *mut i32,
    offsets: *mut i32,
    counts: *mut i32,
    members: *mut i32,
) -> u32 {
    if count < 0 || count as usize > isize::MAX as usize / size_of::<Candidate>() {
        return 0;
    }
    if count == 0 {
        return 1;
    }
    if input.is_null()
        || !input.is_aligned()
        || order.is_null()
        || !order.is_aligned()
        || offsets.is_null()
        || !offsets.is_aligned()
        || counts.is_null()
        || !counts.is_aligned()
        || members.is_null()
        || !members.is_aligned()
    {
        return 0;
    }
    let values = unsafe { slice::from_raw_parts(input, count as usize) };
    let order = unsafe { slice::from_raw_parts_mut(order, count as usize) };
    let offsets = unsafe { slice::from_raw_parts_mut(offsets, count as usize) };
    let counts = unsafe { slice::from_raw_parts_mut(counts, count as usize) };
    let members = unsafe { slice::from_raw_parts_mut(members, count as usize) };
    for (at, value) in order.iter_mut().enumerate() {
        *value = at as i32;
    }
    order.sort_unstable_by(|&a, &b| {
        values[b as usize]
            .priority
            .cmp(&values[a as usize].priority)
            .then_with(|| {
                values[a as usize]
                    .declaration
                    .cmp(&values[b as usize].declaration)
            })
    });
    let mut groups: HashMap<i32, Vec<i32>> = HashMap::new();
    for &at in order.iter() {
        if values[at as usize].pool >= 0 {
            groups.entry(values[at as usize].pool).or_default().push(at);
        }
    }
    let mut cursor = 0;
    for &at in order.iter() {
        if values[at as usize].pool < 0 {
            offsets[at as usize] = cursor as i32;
            counts[at as usize] = 1;
            members[cursor] = at;
            cursor += 1;
            continue;
        }
        if let Some(group) = groups.remove(&values[at as usize].pool) {
            for &member in &group {
                offsets[member as usize] = cursor as i32;
                counts[member as usize] = group.len() as i32;
            }
            members[cursor..cursor + group.len()].copy_from_slice(&group);
            cursor += group.len();
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_follow_full_priority_order_and_stable_declarations() {
        let input = [
            Candidate {
                priority: 1,
                declaration: 0,
                pool: 2,
            },
            Candidate {
                priority: 3,
                declaration: 1,
                pool: -1,
            },
            Candidate {
                priority: 4,
                declaration: 2,
                pool: 2,
            },
            Candidate {
                priority: 3,
                declaration: 3,
                pool: -2,
            },
        ];
        let (mut order, mut offsets, mut counts, mut members) = ([0; 4], [0; 4], [0; 4], [0; 4]);
        assert_eq!(
            unsafe {
                lom_scenario_plan_v1(
                    input.as_ptr(),
                    4,
                    order.as_mut_ptr(),
                    offsets.as_mut_ptr(),
                    counts.as_mut_ptr(),
                    members.as_mut_ptr(),
                )
            },
            1
        );
        assert_eq!(order, [2, 1, 3, 0]);
        assert_eq!(offsets, [0, 2, 0, 3]);
        assert_eq!(counts, [2, 1, 2, 1]);
        assert_eq!(members, [2, 0, 1, 3]);
    }
}
