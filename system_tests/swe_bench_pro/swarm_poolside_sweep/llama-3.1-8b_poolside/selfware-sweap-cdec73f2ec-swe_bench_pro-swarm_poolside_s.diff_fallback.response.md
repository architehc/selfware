diff --git a/src/user/email.js b/src/user/email.js
index 5e6c4a1..f5c8e5c 100644
--- a/src/user/email.js
+++ b/src/user/email.js
@@ -58,6 +58,13 @@ UserEmail.getValidationExpiry = async (uid) => {
 	return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };

+UserEmail.getPendingEmail = async (uid) => {
+	const pending = await UserEmail.isValidationPending(uid);
+	if (pending) {
+		const code = await db.get(`confirm:byUid:${uid}`);
+		const confirmObj = await db.get(`confirm:${code}`);
+		return confirmObj.email;
+	}
+	return null;
+};

 UserEmail.expireValidation = async (uid) => {
 	const code = await db.get(`confirm:byUid:${uid}`);
 	await db.deleteAll([
@@ -71,6 +78,13 @@ UserEmail.canSendValidation = async (uid, email) => {
 	if (!pending) {
 		return true;
 	}

+	const pendingEmail = await UserEmail.getPendingEmail(uid);
+	if (pendingEmail && pendingEmail !== email) {
+		return false;
+	}
+
 	const expiry = await UserEmail.getValidationExpiry(uid);
 	const now = Date.now();
 	const interval = await db.get(`email:validationInterval`);
@@ -83,6 +97,13 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return now + interval < expiry;
 };

+UserEmail.getEmailForValidation = async (uid) => {
+	const email = await UserEmail.getPendingEmail(uid);
+	if (email) {
+		return email;
+	}
+	return await user.email.getEmail(uid);
+};

 UserEmail.sendValidationEmail = async (uid, email) => {
 	const code = await db.generateUUID();
 	const expires = Date.now() + 30 * 24 * 60 * 60 * 1000;
 	const confirmObj = {
@@ -96,6 +117,13 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	await db.set(`confirm:${code}`, confirmObj);
 	await db.set(`confirm:byUid:${uid}`, code);
 	await user.email.setEmail(uid, email);
+	const pendingEmail = await UserEmail.getPendingEmail(uid);
+	if (pendingEmail && pendingEmail !== email) {
+		throw new Error('Email already pending validation');
+	}
+
 	return code;
 };

diff --git a/src/user/user.js b/src/user/user.js
index 5e6c4a1..f5c8e5c 100644
--- a/src/user/user.js
+++ b/src/user/user.js
@@ -54,6 +54,13 @@ User.setEmail = async (uid, email) => {
 	await db.set(`user:${uid}`, { email });
 };

+User.getEmail = async (uid) => {
+	const user = await db.get(`user:${uid}`);
+	return user.email;
+};

 User.validateEmail = async (uid) => {
 	const email = await user.email.getEmailForValidation(uid);
 	if (email) {
@@ -67,6 +74,13 @@ User.validateEmail = async (uid) => {
 	const code = await db.get(`confirm:byUid:${uid}`);
 	const confirmObj = await db.get(`confirm:${code}`);
 	if (confirmObj.expires < Date.now()) {
+		const pendingEmail = await user.email.getEmailForValidation(uid);
+		if (pendingEmail) {
+			await user.email.setEmail(uid, pendingEmail);
+		}
+
 		await db.deleteAll([
 			`confirm:byUid:${uid}`,
 			`confirm:${code}`,
@@ -81,6 +95,13 @@ User.validateEmail = async (uid) => {
 	const code = await db.get(`confirm:byUid:${uid}`);
 	const confirmObj = await db.get(`confirm:${code}`);
 	if (confirmObj.expires < Date.now()) {
+		const pendingEmail = await user.email.getEmailForValidation(uid);
+		if (pendingEmail) {
+			await user.email.setEmail(uid, pendingEmail);
+		}
+
 		await db.deleteAll([
 			`confirm:byUid:${uid}`,
 			`confirm:${code}`,
diff --git a/src/user/email.js b/src/user/email.js
index f5c8e5c..e5c8e5c 100644
--- a/src/user/email.js
+++ b/src/user/email.js
@@ -71,13 +71,13 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return now + interval < expiry;
 };

 UserEmail.getEmailForValidation = async (uid) => {
-	const email = await user.email.getEmail(uid);
+	const pendingEmail = await UserEmail.getPendingEmail(uid);
+	if (pendingEmail) {
+		return pendingEmail;
+	}
+	return await user.email.getEmail(uid);
 };

 UserEmail.sendValidationEmail = async (uid, email) => {
 	const code = await db.generateUUID();
 	const expires = Date.now() + 30 * 24 * 60 * 60 * 1000;
 	const confirmObj = {
@@ -96,6 +96,13 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	await db.set(`confirm:${code}`, confirmObj);
 	await db.set(`confirm:byUid:${uid}`, code);
 	await user.email.setEmail(uid, email);
+	const pendingEmail = await UserEmail.getPendingEmail(uid);
+	if (pendingEmail && pendingEmail !== email) {
+		throw new Error('Email already pending validation');
+	}
+
 	return code;
 };

diff --git a/src/user/email.js b/src/user/email.js
index e5c8e5c..e5c8e5c 100644
--- a/src/user/email.js
+++ b/src/user/email.js
@@ -71,13 +71,13 @@ UserEmail.canSendValidation = async (uid, email) => {
 	return now + interval < expiry;
 };

 UserEmail.getEmailForValidation = async (uid) => {
-	const email = await user.email.getEmail(uid);
+	const pendingEmail = await UserEmail.getPendingEmail(uid);
+	if (pendingEmail) {
+		return pendingEmail;
+	}
+	return await user.email.getEmail(uid);
 };

 UserEmail.sendValidationEmail = async (uid, email) => {
 	const code = await db.generateUUID();
 	const expires = Date.now() + 30 * 24 * 60 * 60 * 1000;
 	const confirmObj = {
@@ -96,6 +96,13 @@ UserEmail.sendValidationEmail = async (uid, email) => {
 	await db.set(`confirm:${code}`, confirmObj);
 	await db.set(`confirm:byUid:${uid}`, code);
 	await user.email.setEmail(uid, email);
+	const pendingEmail = await UserEmail.getPendingEmail(uid);
+	if (pendingEmail && pendingEmail !== email) {
+		throw new Error('Email already pending validation');
+	}
+
 	return code;
 };