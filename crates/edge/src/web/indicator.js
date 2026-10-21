// SPDX-License-Identifier: AGPL-3.0-only
// An optional UI hint: it cannot block configuration or touch messages/calls.
export class ConfigurationIndicator {
 constructor(prepare){this.prepare=prepare;this.view=crypto.randomUUID();this.generation=0;this.queue=Promise.resolve();}
 set(active){
  const generation=++this.generation;
  const send=this.prepare({op:'configuration_indicator',view_id:this.view,active});
  this.queue=this.queue.catch(()=>{}).then(()=>{
   if(generation!==this.generation)return;
   return send();
  });
  return this.queue.catch(()=>{});
 }
}
