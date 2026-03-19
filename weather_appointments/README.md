# ☀️ Sunny Day Appointments

A beautiful, modern web application for managing outdoor appointments based on good weather conditions.

## Features

- 📅 **Visual Calendar** - Display all appointments in a beautiful card-based grid layout
- 🌤️ **Weather Filtering** - Filter by weather type (Sunny, Partly Cloudy, Clear Sky, Warm)
- 📊 **Monthly Stats** - Track total appointments, this month's count, and average temperature
- 🔍 **Smart Filtering** - Filter by month and weather type
- 💾 **Local Storage** - All data is saved in your browser (no backend required)
- 📱 **Responsive Design** - Works beautifully on desktop, tablet, and mobile
- 🎨 **Beautiful UI** - Modern, colorful interface with smooth animations

## Quick Start

### Option 1: Python Server (Recommended)

```bash
cd weather_appointments
python3 server.py
```

Then open your browser to: **http://localhost:8080**

### Option 2: Any Static Server

```bash
# Using npx (Node.js)
npx serve weather_appointments

# Using PHP
cd weather_appointments
php -S localhost:8080

# Using Ruby
cd weather_appointments
ruby -run -ehttpd . -p8080
```

### Option 3: Direct File Access

Simply open `index.html` in your browser (works in most modern browsers)

## Usage

1. **Add Appointments**
   - Click the "+ Add Appointment" button
   - Fill in the details (title, date, time, weather type, temperature)
   - Save and see it appear in the grid

2. **Filter Appointments**
   - Use the "Weather Type" dropdown to filter by specific weather
   - Use the "Month" dropdown to see appointments for a specific month

3. **View Statistics**
   - Top panel shows total appointments, this month's count, and average temperature

4. **Manage Appointments**
   - Click the trash icon on any appointment card to delete it

## Features in Detail

### Weather Types

- ☀️ **Sunny** - Perfect sunny days
- ⛅ **Partly Cloudy** - Mixed sun and clouds
- 🌟 **Clear Sky** - Crystal clear conditions
- 🌡️ **Warm** - Great temperatures for outdoor activities

### Data Persistence

All appointments are stored in your browser's localStorage, so they persist across sessions without needing a backend server or database.

### Responsive Design

The application automatically adapts to different screen sizes:
- Desktop: Multi-column grid layout
- Tablet: Two-column layout
- Mobile: Single column with optimized touch controls

## Project Structure

```
weather_appointments/
├── index.html      # Main HTML file
├── styles.css      # Styling and animations
├── app.js          # Application logic
├── server.py       # Simple Python HTTP server
└── README.md       # This file
```

## Technologies Used

- **HTML5** - Semantic markup
- **CSS3** - Modern styling with gradients, animations, and flexbox/grid
- **Vanilla JavaScript** - No frameworks, pure JS with ES6+ features
- **localStorage API** - Client-side data persistence

## Browser Support

- ✅ Chrome/Edge (recommended)
- ✅ Firefox
- ✅ Safari
- ✅ Opera

## Future Enhancements

Potential features that could be added:
- Weather API integration for automatic weather checking
- Calendar export (iCal, Google Calendar)
- Appointment reminders
- Photo attachments for each appointment
- Share appointments with friends
- Dark mode toggle
- More detailed weather metrics (humidity, wind, etc.)

## License

MIT License - Feel free to use and modify as needed!

---

Enjoy planning your sunny day adventures! 🌞
