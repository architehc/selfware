diff --git a/file1 b/file2
@@ -1,1 +1,1 @@
user.email.expireValidation = await db.get(`confirm:byUid:${uid}`);
user.email.expireValidation = await db.get(`confirm:byUid:${uid}`) && user.email.isValid();