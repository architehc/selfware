diff --git a/src/user/email.js b/src/user/email.js
index 54a1c2d..e3f5b9a 100644
--- a/src/user/email.js
+++ b/src/user/email.js
@@ -58,6 +58,7 @@ UserEmail.getValidationExpiry = async (uid) => {
   const pending = await UserEmail.isValidationPending(uid);
   return pending ? db.pttl(`confirm:byUid:${uid}`) : null;
 };
+UserEmail.getConfirmObjs = async (uids) => {
+  const byUids = await db.mget(uids.map(u => `confirm:byUid:${u}`));
+  const confirmObjs = {};
+  byUids.forEach((code, i) => {
+    if (code) {
+      const confirmObj = await db.getObject(`confirm:${code}`);
+      confirmObjs[uids[i]] = confirmObj;
+    }
+  });
+  return confirmObjs;
+};
 UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
@@ -71,6 +72,7 @@ UserEmail.canSendValidation = async (uid, email) => {
   if (!pending) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() >= confirmObj.expires) {
+    return false;
+  }
   return true;
 };
 
@@ -78,6 +80,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
 };
 
@@ -85,6 +88,7 @@ UserEmail.getEmailForValidation = async (uid) => {
   const profileEmail = await user.getEmail(uid);
   if (profileEmail) {
     return profileEmail;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj) {
+    return confirmObj.email;
+  }
   return null;
 };
 
@@ -92,6 +96,7 @@ UserEmail.isValidationPending = async (uid, email) => {
   const pending = await UserEmail.isValidationPending(uid);
   if (!pending) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires && (email === undefined || email === confirmObj.email)) {
+    return true;
+  }
   return false;
 };
 
@@ -99,6 +104,7 @@ UserEmail.canSendValidation = async (uid, email) => {
   const pending = await UserEmail.isValidationPending(uid, email);
   if (!pending) {
     return true;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    return false;
+  }
   return true;
 };
 
@@ -106,6 +112,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -113,6 +120,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -120,6 +128,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -127,6 +136,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -134,6 +144,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -141,6 +152,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -148,6 +160,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -155,6 +168,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -162,6 +176,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -169,6 +184,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -176,6 +192,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -183,6 +200,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -190,6 +208,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -197,6 +216,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -204,6 +224,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -211,6 +232,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -218,6 +240,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -225,6 +248,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -232,6 +256,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -239,6 +264,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -246,6 +272,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -253,6 +280,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -260,6 +288,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -267,6 +296,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -274,6 +304,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -281,6 +312,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -288,6 +320,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -295,6 +328,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -302,6 +336,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -309,6 +344,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -316,6 +352,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -323,6 +360,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -330,6 +368,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -337,6 +376,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -344,6 +384,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -351,6 +392,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -358,6 +400,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -365,6 +408,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -372,6 +416,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -379,6 +424,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -386,6 +432,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -393,6 +440,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -400,6 +448,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -407,6 +456,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -414,6 +464,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -421,6 +472,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -428,6 +480,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -435,6 +488,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -442,6 +496,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -449,6 +504,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -456,6 +512,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -463,6 +520,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -470,6 +528,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -477,6 +536,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -484,6 +544,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -491,6 +552,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -498,6 +560,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -505,6 +568,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -512,6 +576,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -519,6 +584,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -526,6 +592,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -533,6 +600,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -540,6 +608,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -547,6 +616,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -554,6 +624,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -561,6 +632,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -568,6 +640,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -575,6 +648,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -582,6 +656,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -589,6 +664,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -596,6 +672,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -603,6 +680,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -610,6 +688,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -617,6 +696,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -624,6 +704,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -631,6 +712,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -638,6 +720,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -645,6 +728,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -652,6 +736,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -659,6 +744,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -666,6 +752,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -673,6 +760,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -680,6 +768,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -687,6 +776,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -694,6 +784,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -701,6 +792,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -708,6 +800,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -715,6 +808,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -722,6 +816,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -729,6 +824,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -736,6 +832,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -743,6 +840,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -750,6 +848,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -757,6 +856,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -764,6 +864,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -771,6 +872,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -778,6 +880,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -785,6 +888,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -792,6 +896,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -799,6 +904,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -806,6 +912,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -813,6 +920,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -820,6 +928,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -827,6 +936,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -834,6 +944,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -841,6 +952,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -848,6 +960,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -855,6 +968,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -862,6 +976,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -869,6 +984,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -876,6 +992,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -883,6 +1000,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -890,6 +1008,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -897,6 +1016,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -904,6 +1024,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -911,6 +1032,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -918,6 +1040,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -925,6 +1048,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -932,6 +1056,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -939,6 +1064,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -946,6 +1072,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -953,6 +1080,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -960,6 +1088,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -967,6 +1096,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -974,6 +1104,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -981,6 +1112,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -988,6 +1120,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -995,6 +1128,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -1002,6 +1136,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -1009,6 +1144,7 @@ UserEmail.expireValidation = async (uid) => {
   const code = await db.get(`confirm:byUid:${uid}`);
   await db.deleteAll([
     `confirm:byUid:${uid}`,
     `confirm:${code}`,
   ]);
+  await db.delete(`confirm:byUid:${uid}`);
 };
 
@@ -1016,6 +1152,7 @@ UserEmail.validateEmail = async (uid) => {
   const email = await UserEmail.getEmailForValidation(uid);
   if (!email) {
     return false;
   }
+  const confirmObj = await UserEmail.getConfirmObjs([uid])[uid];
+  if (confirmObj && Date.now() < confirmObj.expires) {
+    await user.setUserField(uid, 'email', email);
+    await db.delete(`confirm:byUid:${uid}`);
+    await db.delete(`confirm:${confirmObj.code}`);
+    return true;
+  }
   return false;
 };
 
@@ -1023,6 +1160,7 @@ UserEmail.sendValidationEmail = async (uid, email) => {
   const code = crypto.randomBytes(16).toString('hex');
   const expires = Date.now() + 1000 * 60 * 60 * 24; // 24 hours
   await db.setObject(`confirm:${code}`, {
     uid,
     email,
     expires,
   });
   await db.set(`confirm:byUid:${uid}`, code);
   // Send email logic here
+  await UserEmail.getConfirmObjs([uid]);
 };
 
@@ -102