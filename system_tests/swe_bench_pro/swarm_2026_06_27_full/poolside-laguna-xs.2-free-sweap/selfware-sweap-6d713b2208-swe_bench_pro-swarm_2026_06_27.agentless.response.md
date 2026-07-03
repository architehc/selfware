
Looking at the issue, I need to:
1. Create `src/controllers/well-known.js` with WebFinger handler
2. Create `src/routes/well-known.js` with the routes
3. Find and remove the change-password route from its current location
4. Integrate the new routes into the app

Let me first check the existing route structure:
<tool_call>shell
<arg_key>command</arg_key>
<arg_value>find src -name "*.js" | head -30</arg_value>
</tool_call>
