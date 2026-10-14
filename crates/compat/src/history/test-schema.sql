-- SPDX-License-Identifier: AGPL-3.0-only
-- Independently authored synthetic schema from documented column contracts.
-- Contains no real credentials or communication records.
CREATE TABLE sms(id INTEGER PRIMARY KEY,request_id TEXT,line_id TEXT,endpoint_line_id TEXT,endpoint_message_id TEXT,imsi TEXT,iccid TEXT,peer TEXT,sender TEXT,recipient TEXT,content TEXT,type INTEGER,status INTEGER,state TEXT,delivery_status TEXT,revision INTEGER,timestamp TEXT,created_at TEXT,deleted_at TEXT);
CREATE TABLE sms_contacts(line_id TEXT,imsi TEXT,iccid TEXT,peer TEXT,last_sms_id INTEGER,last_timestamp TEXT,last_content TEXT,last_type INTEGER,unread_count INTEGER,marked_unread INTEGER,is_favorite INTEGER,created_at TEXT,updated_at TEXT);
CREATE TABLE contacts(id TEXT PRIMARY KEY,owner_user_id TEXT,display_name TEXT,avatar TEXT,notes TEXT,preferred_line_id TEXT,is_favorite INTEGER,revision INTEGER,created_at TEXT,updated_at TEXT);
CREATE TABLE contact_phones(id TEXT PRIMARY KEY,contact_id TEXT,label TEXT,original_number TEXT,canonical_e164 TEXT,region TEXT,is_primary INTEGER);
CREATE TABLE call_history(id TEXT PRIMARY KEY,request_id TEXT,line_id TEXT,endpoint_line_id TEXT,local_phone TEXT,line_imsi TEXT,line_iccid TEXT,direction TEXT,remote_number TEXT,endpoint_id TEXT,endpoint_call_id TEXT,phase TEXT,revision INTEGER,created_at TEXT,updated_at TEXT,active_at TEXT,ended_at TEXT,read_at TEXT,is_favorite INTEGER,end_reason TEXT,failure_code TEXT);
CREATE TABLE modemdeck_user_message_thread_state(user_id TEXT,line_id TEXT,peer TEXT,last_read_sms_id INTEGER,marked_unread INTEGER,is_favorite INTEGER,updated_at TEXT);
CREATE TABLE modemdeck_user_call_state(user_id TEXT,call_id TEXT,is_read INTEGER,is_favorite INTEGER,updated_at TEXT);
CREATE TABLE modemdeck_lines(line_id TEXT PRIMARY KEY,phone_number TEXT,home_country_iso TEXT,line_label TEXT,line_color TEXT,created_at TEXT,updated_at TEXT);
CREATE TABLE modemdeck_legacy_endpoint_lines(endpoint_id TEXT PRIMARY KEY,line_id TEXT,created_at TEXT);
CREATE TABLE modemdeck_user_lines(user_id TEXT,line_id TEXT,assigned_at TEXT);
CREATE TABLE modemdeck_admin_credentials(singleton INTEGER PRIMARY KEY,username TEXT,password_hash TEXT);
INSERT INTO modemdeck_admin_credentials VALUES(1,'synthetic-admin','MUST_NEVER_IMPORT_THIS_AUTH_SECRET');
INSERT INTO sms(id,line_id,peer,sender,content,type,state,revision,deleted_at) VALUES(42,'line_synthetic','+12025550123','+12025550123','合成历史消息',1,'received',7,NULL),(43,'line_synthetic','+12025550123','+12025550123','合成已删除历史',1,'received',8,'2026-10-13T01:00:00Z'),(44,'line_synthetic','+12025550123','','合成历史发送状态',2,'submitted',1,NULL);
INSERT INTO sms_contacts(line_id,peer,last_sms_id,unread_count,marked_unread,is_favorite) VALUES('line_synthetic','+12025550123',44,1,1,1);
INSERT INTO contacts(id,owner_user_id,display_name,notes,preferred_line_id,revision) VALUES('contact_synthetic','legacy_user_a','合成联系人','合成备注','line_synthetic',3);
INSERT INTO contact_phones VALUES('phone_synthetic','contact_synthetic','home','2025550123','+12025550123','US',1);
INSERT INTO call_history(id,line_id,direction,remote_number,endpoint_call_id,phase,revision,read_at) VALUES('call_synthetic','line_synthetic','incoming','+12025550123','legacy-call-1','terminated',9,'2026-10-13T02:00:00Z');
INSERT INTO modemdeck_user_message_thread_state VALUES('legacy_user_a','line_synthetic','+12025550123',42,0,1,NULL),('legacy_user_b','line_synthetic','+12025550123',0,1,0,NULL);
INSERT INTO modemdeck_user_call_state VALUES('legacy_user_a','call_synthetic',1,1,NULL),('legacy_user_b','call_synthetic',0,0,NULL);
INSERT INTO modemdeck_lines(line_id,line_label) VALUES('line_synthetic','合成线路');
INSERT INTO modemdeck_legacy_endpoint_lines(endpoint_id,line_id) VALUES('legacy-device-synthetic','line_synthetic');
INSERT INTO modemdeck_user_lines VALUES('legacy_user_a','line_synthetic',NULL),('legacy_user_b','line_synthetic',NULL);
