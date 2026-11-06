//! Le tri de Sort-Object, copié du bloc jeux (les blocs ne dépendent pas les uns des autres) : l'ancien tri rapide
//! de .NET Framework, pas stable ; reproduit le même ordre pour les éléments égaux.

use std::cmp::Ordering;

/// Le tri de Sort-Object (l'ancien tri rapide de .NET Framework, pas stable). Reproduit le même ordre pour les
/// éléments égaux.
pub fn trier_ps<T: Clone, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], mut cmp: F) {
    let n = v.len();
    if n < 2 {
        return;
    }
    rapide(v, 0, n as isize - 1, 32, &mut cmp);
}

fn plus_grand<T, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], a: isize, b: isize, cmp: &mut F) {
    if a != b && cmp(&v[a as usize], &v[b as usize]) == Ordering::Greater {
        v.swap(a as usize, b as usize);
    }
}

fn tas<T, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], lo: isize, hi: isize, cmp: &mut F) {
    let n = hi - lo + 1;
    let mut i = n >> 1;
    while i >= 1 {
        descendre(v, i, n, lo, cmp);
        i -= 1;
    }
    let mut i = n;
    while i > 1 {
        v.swap(lo as usize, (lo + i - 1) as usize);
        descendre(v, 1, i - 1, lo, cmp);
        i -= 1;
    }
}

fn descendre<T, F: FnMut(&T, &T) -> Ordering>(v: &mut [T], mut i: isize, n: isize, lo: isize, cmp: &mut F) {
    while i <= n >> 1 {
        let mut enfant = 2 * i;
        if enfant < n && cmp(&v[(lo + enfant - 1) as usize], &v[(lo + enfant) as usize]) == Ordering::Less {
            enfant += 1;
        }
        if cmp(&v[(lo + i - 1) as usize], &v[(lo + enfant - 1) as usize]) != Ordering::Less {
            break;
        }
        v.swap((lo + i - 1) as usize, (lo + enfant - 1) as usize);
        i = enfant;
    }
}

/// L'ancien tri rapide de .NET (DepthLimitedQuickSort), celui de Windows PowerShell 5.1.
fn rapide<T: Clone, F: FnMut(&T, &T) -> Ordering>(
    v: &mut [T],
    mut gauche: isize,
    mut droite: isize,
    mut prof: i64,
    cmp: &mut F,
) {
    loop {
        if prof == 0 {
            tas(v, gauche, droite, cmp);
            return;
        }
        let (mut i, mut j) = (gauche, droite);
        let milieu = i + ((j - i) >> 1);
        plus_grand(v, i, milieu, cmp);
        plus_grand(v, i, j, cmp);
        plus_grand(v, milieu, j, cmp);
        let x = v[milieu as usize].clone();
        loop {
            while cmp(&v[i as usize], &x) == Ordering::Less {
                i += 1;
            }
            while cmp(&x, &v[j as usize]) == Ordering::Less {
                j -= 1;
            }
            if i > j {
                break;
            }
            if i < j {
                v.swap(i as usize, j as usize);
            }
            i += 1;
            j -= 1;
            if i > j {
                break;
            }
        }
        prof -= 1;
        if j - gauche <= droite - i {
            if gauche < j {
                rapide(v, gauche, j, prof, cmp);
            }
            gauche = i;
        } else {
            if i < droite {
                rapide(v, i, droite, prof, cmp);
            }
            droite = j;
        }
        if gauche >= droite {
            return;
        }
    }
}
