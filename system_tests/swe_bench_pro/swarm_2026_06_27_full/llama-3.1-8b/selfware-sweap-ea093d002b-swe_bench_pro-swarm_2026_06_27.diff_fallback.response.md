diff --git a/src/user/email.js b/src/user/email.js
index 54d1b3f..a4c3f1c 100644
--- a/src/user/email.js
+++ b/src/user/email.js
@@ -58,7 +58,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	return pending? db.pttl(`confirm:byUid:${uid}`) : null;
 };

 UserEmail.expireValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	await db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
@@ -71,7 +71,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
 	if (!pending) {

 UserEmail.getEmailForValidation = async (uid) => {
-	return user.email.getEmailForValidation(uid);
+	const email = await user.email.getEmailForValidation(uid);
+	return email || (await db.mget(['confirm:byUid:' + uid])[0])?.email;
 };

 UserEmail.isValidationPending = async (uid, email) => {
@@ -83,7 +84,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) return false;
 	const expires = await db.pttl(`confirm:${code}`);
-	return expires > 0 && (email? email === code.email : true);
+	return expires > 0 && (email? email === code.email : true) && code.expires > Date.now();
 };

 UserEmail.sendValidationEmail = async (uid, email) => {
@@ -96,7 +97,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (code) {
 		const expires = code.expires;
-		db.set(`confirm:${code}`, {...code, expires: expires + 1000 * 60 * 60 * 24 });
+		db.set(`confirm:${code}`, {...code, expires: expires + 1000 * 60 * 60 * 24 });
 		return;
 	}
 	const newCode = await db.generateId();
@@ -109,7 +110,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:byUid:${uid}`, code);
 	db.set(`confirm:${code}`, { email, expires: Date.now() + 1000 * 60 * 60 * 24 });
 };

diff --git a/src/user/main.js b/src/user/main.js
index 123456..789012 100644
--- a/src/user/main.js
+++ b/src/user/main.js
@@ -20,6 +20,7 @@ const db = require('./db');
 const User = require('./user');
 const UserEmail = require('./email');

+db.mget = async (keys) => {
+	return Promise.all(keys.map(key => db.get(key))).then(values => values.filter(value => value!== null));
+};

 module.exports = User;
 
diff --git a/src/user/db.js b/src/user/db.js
index 123456..789012 100644
--- a/src/user/db.js
+++ b/src/user/db.js
@@ -20,6 +20,7 @@ const db = require('./db');
 const User = require('./user');
 const UserEmail = require('./email');

+db.mget = async (keys) => {
+	return Promise.all(keys.map(key => db.get(key))).then(values => values.filter(value => value!== null));
+};

 module.exports = db;
 
diff --git a/src/user/email/main.js b/src/user/email/main.js
index 123456..789012 100644
--- a/src/user/email/main.js
+++ b/src/user/email/main.js
@@ -20,6 +20,7 @@ const db = require('./db');
 const UserEmail = require('./email');

+db.mget = async (keys) => {
+	return Promise.all(keys.map(key => db.get(key))).then(values => values.filter(value => value!== null));
+};

 module.exports = db;
 
diff --git a/src/user/email/db.js b/src/user/email/db.js
index 123456..789012 100644
--- a/src/user/email/db.js
+++ b/src/user/email/db.js
@@ -20,6 +20,7 @@ const db = require('./db');
 const UserEmail = require('./email');

+db.mget = async (keys) => {
+	return Promise.all(keys.map(key => db.get(key))).then(values => values.filter(value => value!== null));
+};

 module.exports = db;
 
diff --git a/src/user/email/redis.js b/src/user/email/redis.js
index 123456..789012 100644
--- a/src/user/email/redis.js
+++ b/src/user/email/redis.js
@@ -20,6 +20,7 @@ const redis = require('./redis');
 const UserEmail = require('./email');

+redis.mget = async (keys) => {
+	return await redis.client.mget(keys).then(values => values.filter(value => value!== null));
+};

 module.exports = redis;
 
diff --git a/src/user/email/mongo.js b/src/user/email/mongo.js
index 123456..789012 100644
--- a/src/user/email/mongo.js
+++ b/src/user/email/mongo.js
@@ -20,6 +20,7 @@ const mongo = require('./mongo');
 const UserEmail = require('./email');

+mongo.mget = async (keys) => {
+	return await mongo.db.collection('objects').find({ _key: { $in: keys } }).toArray().then(values => values.map(value => value.value).filter(value => value!== null));
+};

 module.exports = mongo;
 
diff --git a/src/user/email/postgres.js b/src/user/email/postgres.js
index 123456..789012 100644
--- a/src/user/email/postgres.js
+++ b/src/user/email/postgres.js
@@ -20,6 +20,7 @@ const postgres = require('./postgres');
 const UserEmail = require('./email');

+postgres.mget = async (keys) => {
+	return await postgres.db.query(`SELECT value FROM legacy_string WHERE key IN ($1)`, [keys]).then(values => values.rows.map(value => value.value).filter(value => value!== null));
+};

 module.exports = postgres;