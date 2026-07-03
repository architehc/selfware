diff --git a/src/user/email.js b/src/user/email.js
index 5f3d1a1..a5f2d3c 100644
--- a/src/user/email.js
+++ b/src/user/email.js
@@ -58,7 +58,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
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
 		const expiry = await UserEmail.getValidationExpiry(uid);
-		if (expiry > Date.now()) {
+		if (expiry !== null && expiry > Date.now()) {
 			return true;
 		}
 	}
@@ -83,7 +83,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
@@ -97,7 +97,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	const user = await db.get(`user:${uid}`);
 	if (user.email) {
 		return user.email;
 	}
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -111,7 +111,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -125,7 +125,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -141,7 +141,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -155,7 +155,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -173,7 +173,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -191,7 +191,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -209,7 +209,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -227,7 +227,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -243,7 +243,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -259,7 +259,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -273,7 +273,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -287,7 +287,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -301,7 +301,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -315,7 +315,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -329,7 +329,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -343,7 +343,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -359,7 +359,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -373,7 +373,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -391,7 +391,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -409,7 +409,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -427,7 +427,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -445,7 +445,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -461,7 +461,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -477,7 +477,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -491,7 +491,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -505,7 +505,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -519,7 +519,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -533,7 +533,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -547,7 +547,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -561,7 +561,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -577,7 +577,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -591,7 +591,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -609,7 +609,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -627,7 +627,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -645,7 +645,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -663,7 +663,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -679,7 +679,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -695,7 +695,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -709,7 +709,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -723,7 +723,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -737,7 +737,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -751,7 +751,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -765,7 +765,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -779,7 +779,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -795,7 +795,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -809,7 +809,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -827,7 +827,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -845,7 +845,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -863,7 +863,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -881,7 +881,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -897,7 +897,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -913,7 +913,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -927,7 +927,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -941,7 +941,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -955,7 +955,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -969,7 +969,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -983,7 +983,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -997,7 +997,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -1013,7 +1013,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -1027,7 +1027,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1045,7 +1045,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -1063,7 +1063,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1081,7 +1081,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1099,7 +1099,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -1115,7 +1115,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -1131,7 +1131,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -1145,7 +1145,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -1159,7 +1159,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -1173,7 +1173,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -1187,7 +1187,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -1201,7 +1201,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -1215,7 +1215,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -1231,7 +1231,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -1245,7 +1245,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1263,7 +1263,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -1281,7 +1281,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1299,7 +1299,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1317,7 +1317,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -1333,7 +1333,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -1349,7 +1349,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -1363,7 +1363,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -1377,7 +1377,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -1391,7 +1391,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -1405,7 +1405,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -1419,7 +1419,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -1433,7 +1433,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -1449,7 +1449,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -1463,7 +1463,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1481,7 +1481,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -1499,7 +1499,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1517,7 +1517,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1535,7 +1535,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -1551,7 +1551,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -1567,7 +1567,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -1581,7 +1581,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -1595,7 +1595,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -1609,7 +1609,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -1623,7 +1623,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -1637,7 +1637,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -1651,7 +1651,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -1667,7 +1667,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -1681,7 +1681,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1699,7 +1699,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -1717,7 +1717,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1735,7 +1735,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1753,7 +1753,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -1769,7 +1769,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -1785,7 +1785,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -1799,7 +1799,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -1813,7 +1813,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -1827,7 +1827,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -1841,7 +1841,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -1855,7 +1855,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -1869,7 +1869,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -1885,7 +1885,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -1899,7 +1899,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1917,7 +1917,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -1935,7 +1935,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1953,7 +1953,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -1971,7 +1971,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -1987,7 +1987,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -2003,7 +2003,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -2017,7 +2017,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -2031,7 +2031,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -2045,7 +2045,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -2059,7 +2059,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -2073,7 +2073,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -2087,7 +2087,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -2103,7 +2103,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -2117,7 +2117,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2135,7 +2135,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -2153,7 +2153,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2171,7 +2171,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2189,7 +2189,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -2205,7 +2205,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -2221,7 +2221,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -2235,7 +2235,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -2249,7 +2249,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -2263,7 +2263,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -2277,7 +2277,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -2291,7 +2291,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -2305,7 +2305,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -2321,7 +2321,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -2335,7 +2335,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2353,7 +2353,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -2371,7 +2371,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2389,7 +2389,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2407,7 +2407,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -2423,7 +2423,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -2439,7 +2439,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -2453,7 +2453,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -2467,7 +2467,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -2481,7 +2481,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -2495,7 +2495,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -2509,7 +2509,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -2523,7 +2523,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -2539,7 +2539,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -2553,7 +2553,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2571,7 +2571,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -2589,7 +2589,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2607,7 +2607,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2625,7 +2625,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -2641,7 +2641,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -2657,7 +2657,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -2671,7 +2671,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -2685,7 +2685,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -2699,7 +2699,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -2713,7 +2713,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -2727,7 +2727,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -2741,7 +2741,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -2757,7 +2757,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -2771,7 +2771,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2789,7 +2789,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -2807,7 +2807,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2825,7 +2825,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -2843,7 +2843,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -2859,7 +2859,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -2875,7 +2875,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -2889,7 +2889,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -2903,7 +2903,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -2917,7 +2917,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -2931,7 +2931,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -2945,7 +2945,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -2959,7 +2959,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -2975,7 +2975,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -2989,7 +2989,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3007,7 +3007,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -3025,7 +3025,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3043,7 +3043,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3061,7 +3061,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -3077,7 +3077,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -3093,7 +3093,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -3107,7 +3107,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -3121,7 +3121,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -3135,7 +3135,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -3149,7 +3149,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -3163,7 +3163,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -3177,7 +3177,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -3193,7 +3193,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -3207,7 +3207,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3225,7 +3225,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -3243,7 +3243,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3261,7 +3261,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3279,7 +3279,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -3295,7 +3295,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -3311,7 +3311,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -3325,7 +3325,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -3339,7 +3339,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -3353,7 +3353,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -3367,7 +3367,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -3381,7 +3381,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -3395,7 +3395,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -3411,7 +3411,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -3425,7 +3425,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3443,7 +3443,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -3461,7 +3461,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3479,7 +3479,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3497,7 +3497,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -3513,7 +3513,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -3529,7 +3529,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -3543,7 +3543,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -3557,7 +3557,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -3571,7 +3571,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -3585,7 +3585,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -3599,7 +3599,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -3613,7 +3613,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -3629,7 +3629,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -3643,7 +3643,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3661,7 +3661,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -3679,7 +3679,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3697,7 +3697,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3715,7 +3715,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -3731,7 +3731,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -3747,7 +3747,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -3761,7 +3761,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -3775,7 +3775,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -3789,7 +3789,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -3803,7 +3803,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -3817,7 +3817,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -3831,7 +3831,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -3847,7 +3847,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -3861,7 +3861,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3879,7 +3879,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -3897,7 +3897,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3915,7 +3915,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -3933,7 +3933,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -3949,7 +3949,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -3965,7 +3965,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -3979,7 +3979,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -3993,7 +3993,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -4007,7 +4007,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -4021,7 +4021,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -4035,7 +4035,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -4049,7 +4049,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -4065,7 +4065,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -4079,7 +4079,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4097,7 +4097,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -4115,7 +4115,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4133,7 +4133,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4151,7 +4151,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -4167,7 +4167,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -4183,7 +4183,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -4197,7 +4197,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -4211,7 +4211,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -4225,7 +4225,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -4239,7 +4239,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -4253,7 +4253,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -4267,7 +4267,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -4283,7 +4283,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -4297,7 +4297,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4315,7 +4315,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -4333,7 +4333,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4351,7 +4351,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4369,7 +4369,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -4385,7 +4385,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -4401,7 +4401,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -4415,7 +4415,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -4429,7 +4429,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -4443,7 +4443,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -4457,7 +4457,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -4471,7 +4471,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -4485,7 +4485,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -4501,7 +4501,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -4515,7 +4515,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4533,7 +4533,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -4551,7 +4551,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4569,7 +4569,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4587,7 +4587,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -4603,7 +4603,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -4619,7 +4619,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -4633,7 +4633,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -4647,7 +4647,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -4661,7 +4661,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -4675,7 +4675,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -4689,7 +4689,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -4703,7 +4703,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -4719,7 +4719,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -4733,7 +4733,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4751,7 +4751,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -4769,7 +4769,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4787,7 +4787,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4805,7 +4805,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -4821,7 +4821,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -4837,7 +4837,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -4851,7 +4851,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -4865,7 +4865,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -4879,7 +4879,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -4893,7 +4893,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -4907,7 +4907,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -4921,7 +4921,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -4937,7 +4937,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -4951,7 +4951,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -4969,7 +4969,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -4987,7 +4987,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5005,7 +5005,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5023,7 +5023,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -5039,7 +5039,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -5055,7 +5055,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -5069,7 +5069,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -5083,7 +5083,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -5097,7 +5097,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -5111,7 +5111,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -5125,7 +5125,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -5139,7 +5139,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -5155,7 +5155,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -5169,7 +5169,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5187,7 +5187,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -5205,7 +5205,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5223,7 +5223,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5241,7 +5241,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -5257,7 +5257,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -5273,7 +5273,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -5287,7 +5287,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -5301,7 +5301,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -5315,7 +5315,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -5329,7 +5329,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -5343,7 +5343,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -5357,7 +5357,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -5373,7 +5373,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -5387,7 +5387,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5405,7 +5405,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -5423,7 +5423,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5441,7 +5441,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5459,7 +5459,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -5475,7 +5475,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -5491,7 +5491,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -5505,7 +5505,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -5519,7 +5519,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -5533,7 +5533,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -5547,7 +5547,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -5561,7 +5561,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -5575,7 +5575,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -5591,7 +5591,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -5605,7 +5605,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5623,7 +5623,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -5641,7 +5641,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5659,7 +5659,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5677,7 +5677,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -5693,7 +5693,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -5709,7 +5709,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -5723,7 +5723,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -5737,7 +5737,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -5751,7 +5751,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -5765,7 +5765,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -5779,7 +5779,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -5793,7 +5793,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -5809,7 +5809,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -5823,7 +5823,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5841,7 +5841,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -5859,7 +5859,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5877,7 +5877,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -5895,7 +5895,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -5911,7 +5911,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -5927,7 +5927,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -5941,7 +5941,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -5955,7 +5955,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -5969,7 +5969,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -5983,7 +5983,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -5997,7 +5997,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -6011,7 +6011,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -6027,7 +6027,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -6041,7 +6041,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6059,7 +6059,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -6077,7 +6077,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6095,7 +6095,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6113,7 +6113,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -6129,7 +6129,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -6145,7 +6145,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -6159,7 +6159,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -6173,7 +6173,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -6187,7 +6187,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -6201,7 +6201,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -6215,7 +6215,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -6229,7 +6229,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -6245,7 +6245,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -6259,7 +6259,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6277,7 +6277,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -6295,7 +6295,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6313,7 +6313,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6331,7 +6331,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -6347,7 +6347,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -6363,7 +6363,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -6377,7 +6377,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -6391,7 +6391,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -6405,7 +6405,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -6419,7 +6419,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -6433,7 +6433,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -6447,7 +6447,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -6463,7 +6463,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -6477,7 +6477,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6495,7 +6495,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -6513,7 +6513,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6531,7 +6531,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6549,7 +6549,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -6565,7 +6565,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -6581,7 +6581,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -6595,7 +6595,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -6609,7 +6609,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -6623,7 +6623,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -6637,7 +6637,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -6651,7 +6651,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -6665,7 +6665,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -6681,7 +6681,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -6695,7 +6695,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6713,7 +6713,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -6731,7 +6731,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6749,7 +6749,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6767,7 +6767,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -6783,7 +6783,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -6799,7 +6799,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -6813,7 +6813,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -6827,7 +6827,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -6841,7 +6841,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -6855,7 +6855,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -6869,7 +6869,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -6883,7 +6883,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -6899,7 +6899,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -6913,7 +6913,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6931,7 +6931,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -6949,7 +6949,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6967,7 +6967,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -6985,7 +6985,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -7001,7 +7001,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -7017,7 +7017,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -7031,7 +7031,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -7045,7 +7045,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -7059,7 +7059,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -7073,7 +7073,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -7087,7 +7087,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -7101,7 +7101,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -7117,7 +7117,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -7131,7 +7131,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7149,7 +7149,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -7167,7 +7167,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7185,7 +7185,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7203,7 +7203,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -7219,7 +7219,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -7235,7 +7235,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -7249,7 +7249,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -7263,7 +7263,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -7277,7 +7277,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -7291,7 +7291,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -7305,7 +7305,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -7319,7 +7319,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -7335,7 +7335,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -7349,7 +7349,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7367,7 +7367,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -7385,7 +7385,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7403,7 +7403,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7421,7 +7421,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -7437,7 +7437,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -7453,7 +7453,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -7467,7 +7467,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -7481,7 +7481,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -7495,7 +7495,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -7509,7 +7509,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -7523,7 +7523,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -7537,7 +7537,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -7553,7 +7553,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -7567,7 +7567,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7585,7 +7585,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -7603,7 +7603,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7621,7 +7621,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7639,7 +7639,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -7655,7 +7655,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -7671,7 +7671,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -7685,7 +7685,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -7699,7 +7699,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -7713,7 +7713,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -7727,7 +7727,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -7741,7 +7741,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -7755,7 +7755,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -7771,7 +7771,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -7785,7 +7785,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7803,7 +7803,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -7821,7 +7821,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7839,7 +7839,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -7857,7 +7857,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -7873,7 +7873,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -7889,7 +7889,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -7903,7 +7903,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -7917,7 +7917,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -7931,7 +7931,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -7945,7 +7945,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -7959,7 +7959,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -7973,7 +7973,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -7989,7 +7989,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -8003,7 +8003,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8021,7 +8021,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -8039,7 +8039,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8057,7 +8057,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8075,7 +8075,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -8091,7 +8091,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -8107,7 +8107,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -8121,7 +8121,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -8135,7 +8135,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -8149,7 +8149,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -8163,7 +8163,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -8177,7 +8177,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -8191,7 +8191,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -8207,7 +8207,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -8221,7 +8221,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8239,7 +8239,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -8257,7 +8257,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8275,7 +8275,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8293,7 +8293,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -8309,7 +8309,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -8325,7 +8325,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -8339,7 +8339,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -8353,7 +8353,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -8367,7 +8367,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -8381,7 +8381,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -8395,7 +8395,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -8409,7 +8409,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -8425,7 +8425,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -8439,7 +8439,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8457,7 +8457,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -8475,7 +8475,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8493,7 +8493,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8511,7 +8511,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -8527,7 +8527,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -8543,7 +8543,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -8557,7 +8557,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -8571,7 +8571,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -8585,7 +8585,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -8599,7 +8599,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -8613,7 +8613,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -8627,7 +8627,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -8643,7 +8643,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -8657,7 +8657,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8675,7 +8675,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -8693,7 +8693,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8711,7 +8711,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8729,7 +8729,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -8745,7 +8745,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -8761,7 +8761,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -8775,7 +8775,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -8789,7 +8789,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -8803,7 +8803,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -8817,7 +8817,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -8831,7 +8831,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -8845,7 +8845,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -8861,7 +8861,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -8875,7 +8875,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8893,7 +8893,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -8911,7 +8911,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8929,7 +8929,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -8947,7 +8947,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -8963,7 +8963,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -8979,7 +8979,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -8993,7 +8993,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -9007,7 +9007,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -9021,7 +9021,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -9035,7 +9035,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -9049,7 +9049,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -9063,7 +9063,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -9079,7 +9079,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -9093,7 +9093,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9111,7 +9111,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -9129,7 +9129,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9147,7 +9147,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9165,7 +9165,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -9181,7 +9181,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -9197,7 +9197,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -9211,7 +9211,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -9225,7 +9225,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -9239,7 +9239,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -9253,7 +9253,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -9267,7 +9267,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -9281,7 +9281,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -9297,7 +9297,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -9311,7 +9311,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9329,7 +9329,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -9347,7 +9347,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9365,7 +9365,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9383,7 +9383,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -9399,7 +9399,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -9415,7 +9415,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -9429,7 +9429,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -9443,7 +9443,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -9457,7 +9457,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -9471,7 +9471,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -9485,7 +9485,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -9499,7 +9499,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -9515,7 +9515,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -9529,7 +9529,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9547,7 +9547,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -9565,7 +9565,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9583,7 +9583,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9601,7 +9601,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -9617,7 +9617,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -9633,7 +9633,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -9647,7 +9647,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -9661,7 +9661,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -9675,7 +9675,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -9689,7 +9689,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -9703,7 +9703,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -9717,7 +9717,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -9733,7 +9733,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -9747,7 +9747,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9765,7 +9765,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -9783,7 +9783,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9801,7 +9801,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9819,7 +9819,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -9835,7 +9835,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -9851,7 +9851,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -9865,7 +9865,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -9879,7 +9879,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -9893,7 +9893,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -9907,7 +9907,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -9921,7 +9921,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -9935,7 +9935,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -9951,7 +9951,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -9965,7 +9965,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -9983,7 +9983,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -10001,7 +10001,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -10019,7 +10019,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -10037,7 +10037,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -10053,7 +10053,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -10069,7 +10069,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async (uid) => {
-	const pending = await UserEmail.isValidationPending(uid);
+	const pending = await UserEmail.isValidationPending(uid);
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
 
@@ -10083,7 +10083,7 @@ UserEmail.getValidationExpiry = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.expireValidation = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
@@ -10097,7 +10097,7 @@ UserEmail.expireValidation = async (uid) => {
 	db.deleteAll([
 		`confirm:byUid:${uid}`,
 		`confirm:${code}`,
 	]);
-};
+};
 
 UserEmail.canSendValidation = async (uid, email) => {
 	const pending = await UserEmail.isValidationPending(uid, email);
@@ -10111,7 +10111,7 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return true;
 };
 
 UserEmail.isValidationPending = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
@@ -10125,7 +10125,7 @@ UserEmail.isValidationPending = async (uid, email) => {
 	return confirmObj.expires > Date.now() && (email ? confirmObj.email === email : true);
 };
 
 UserEmail.getEmailForValidation = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -10139,7 +10139,7 @@ UserEmail.getEmailForValidation = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.sendValidationEmail = async (uid, email) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	const expires = Date.now() + 3600000; // 1 hour
 	const confirmObj = {
 		uid,
@@ -10153,7 +10153,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	db.set(`confirm:${code}`, confirmObj);
 	db.set(`confirm:byUid:${uid}`, code);
 	return code;
-};
+};
 
 UserEmail.validateEmail = async (uid) => {
 	const email = await UserEmail.getEmailForValidation(uid);
@@ -10169,7 +10169,7 @@ UserEmail.validateEmail = async (uid) => {
 	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return false;
 	}
-	const confirmObj = await db.getObjects(`confirm:${code}`);
+	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
 	return confirmObj.expires > Date.now();
 };
 
@@ -10183,7 +10183,7 @@ UserEmail.validateEmail = async (uid) => {
 	db.set(`user:${uid}`, user);
 	return true;
 };
 
 UserEmail.getConfirmObjs = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -10201,7 +10201,7 @@ UserEmail.getConfirmObjs = async () => {
 	return result;
 };
 
 UserEmail.loadUserInfo = async (callerUid, uids) => {
-	const confirmObjs = await UserEmail.getConfirmObjs();
+	const confirmObjs = await UserEmail.getConfirmObjs();
 	const users = await db.getObjects(uids.join(','));
 	for (const user of users) {
 		const uid = user._id;
@@ -10219,7 +10219,7 @@ UserEmail.loadUserInfo = async (callerUid, uids) => {
 	db.set(`user:${uid}`, user);
 }
 }
 
 UserEmail.getPendingEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -10237,7 +10237,7 @@ UserEmail.getPendingEmails = async () => {
 	return result;
 };
 
 UserEmail.getExpiredEmails = async () => {
-	const codes = await db.mget(['confirm:byUid:*']);
+	const codes = await db.mget(['confirm:byUid:*']).then(arr => arr.filter(x => x !== null));
 	const confirmObjs = await db.getObjects(codes.join(','));
 	const result = {};
 	for (const code of codes) {
@@ -10255,7 +10255,7 @@ UserEmail.getExpiredEmails = async () => {
 	return result;
 };
 
 UserEmail.getValidationStatus = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return 'missing';
 	}
 	const confirmObj = await db.getObjects(`confirm:${code}`)[0];
@@ -10271,7 +10271,7 @@ UserEmail.getValidationStatus = async (uid) => {
 	return 'expired';
 };
 
 UserEmail.getValidationEmail = async (uid) => {
-	const code = await db.get(`confirm:byUid:${uid}`);
+	const code = await db.mget(['confirm:byUid:' + uid])[0];
 	if (!code) {
 		return null;
 	}
@@ -10287,7 +10287,7 @@ UserEmail.getValidationEmail = async (uid) => {
 	return confirmObj.email;
 };
 
 UserEmail.getValidationExpiry = async