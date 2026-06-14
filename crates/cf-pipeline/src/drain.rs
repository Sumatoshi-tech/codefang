//! `signal_on_drain` channel fan-through with exhaustion signalling.

use crossbeam_channel::Receiver;
use std::thread;

/// Forwards items from `src` to the returned `forwarded` receiver and closes
/// the returned `drained` receiver once `src` is exhausted.
///
/// This enables ending pipeline-stage spans independently.
///
/// A worker thread loops over `src`, forwarding every item to `out`; on
/// exhaustion it drops both the `out` sender (closing the forwarded channel)
/// and the `sig` sender (closing the drained channel) — in crossbeam,
/// dropping a [`Sender`](crossbeam_channel::Sender) closes its channel.
///
/// The drained channel carries `()` and is never sent to — receivers observe
/// closure via `recv()` returning `Err`.
#[must_use]
pub fn signal_on_drain<T: Send + 'static>(
    src: Receiver<T>,
) -> (Receiver<T>, Receiver<()>) {
    let (out_tx, out_rx) = crossbeam_channel::unbounded::<T>();
    let (sig_tx, sig_rx) = crossbeam_channel::bounded::<()>(0);

    thread::spawn(move || {
        // `sig_tx` and `out_tx` are moved in; dropping them at scope exit
        // closes both channels.
        let _sig_guard = sig_tx;
        // `recv()` returns `Err` once `src` is closed and drained.
        while let Ok(item) = src.recv() {
            // If the consumer dropped `out_rx`, send fails; the downstream
            // stage has gone away, so stop forwarding.
            if out_tx.send(item).is_err() {
                break;
            }
        }
        // out_tx dropped here → forwarded channel closes;
        // _sig_guard dropped here → drained channel closes.
    });

    (out_rx, sig_rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_all_items_then_drains() {
        let (tx, rx) = crossbeam_channel::unbounded();
        for i in 0..5 {
            tx.send(i).unwrap();
        }
        drop(tx); // close src

        let (forwarded, drained) = signal_on_drain(rx);

        let mut got = Vec::new();
        while let Ok(v) = forwarded.recv() {
            got.push(v);
        }
        assert_eq!(got, vec![0, 1, 2, 3, 4]);

        // Drained channel must be closed (recv returns Err) after src exhausted.
        assert!(drained.recv().is_err());
    }

    #[test]
    fn empty_source_drains_immediately() {
        let (tx, rx) = crossbeam_channel::unbounded::<i32>();
        drop(tx);

        let (forwarded, drained) = signal_on_drain(rx);
        assert!(forwarded.recv().is_err(), "no items forwarded");
        assert!(drained.recv().is_err(), "drain signalled");
    }

    #[test]
    fn forwarded_channel_closes_after_drain() {
        let (tx, rx) = crossbeam_channel::unbounded();
        tx.send(7).unwrap();
        drop(tx);

        let (forwarded, _drained) = signal_on_drain(rx);
        assert_eq!(forwarded.recv(), Ok(7));
        // After the single item, forwarded must be closed.
        assert!(forwarded.recv().is_err());
    }
}
