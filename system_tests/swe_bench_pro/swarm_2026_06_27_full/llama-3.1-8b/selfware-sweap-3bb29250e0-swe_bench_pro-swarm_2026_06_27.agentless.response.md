### FILE: test/controllers.js
<<<<<<< SEARCH
it('should error if resource parameter is missing', async () => {
	const response = await requestAsync(`${nconf.get('url')}/.well-known/webfinger`, {
		json: true,
		simple: false,
		resolveWithFullResponse: true,
	});
>>>>>>> REPLACE
### FILE: src/controllers/well-known.js
<<<<<<< SEARCH
module.exports = function (router) {
	router.get("/.well-known/webfinger", function (req, res, next) {
		const resource = req.query.resource;
		if (!resource) {
			return res.status(400).json({ error: 'missing resource parameter' });
		}
		//... rest of the handler logic...
	});
};
>>>>>>> REPLACE