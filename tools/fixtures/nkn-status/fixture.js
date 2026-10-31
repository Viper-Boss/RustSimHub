// SPDX-License-Identifier: AGPL-3.0-only
// Test page only; no production backend or mock selector is shipped with Edge.
import {NknStatusPanel} from '/nkn-status.js';
new NknStatusPanel({document,root:document.getElementById('unavailable')});
window.synthetic={calls:0,data:{contract:1,source:'mock',generation:3,sequence:1,age_ms:0,enabled:'on',service:'running',path:'unknown',dht:'unknown',rx_bytes:'18446744073709551615',tx_bytes:'0',rtt_ms:null,peer_count:2,last_error:null}};
window.panel=new NknStatusPanel({document,root:document.getElementById('status'),readStatus:async()=>{synthetic.calls++;return structuredClone(synthetic.data);}});
