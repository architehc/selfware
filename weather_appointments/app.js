// Weather Appointments Application
class WeatherAppointments {
    constructor() {
        this.appointments = this.loadFromStorage();
        this.init();
    }

    init() {
        this.cacheElements();
        this.bindEvents();
        this.renderAppointments();
        this.updateStats();
        // Show status message to indicate app is ready
        setTimeout(() => {
            const statusMsg = document.getElementById('status-message');
            if (statusMsg) {
                statusMsg.style.display = 'block';
                // Auto-hide after 5 seconds
                setTimeout(() => {
                    statusMsg.style.display = 'none';
                }, 5000);
            }
        }, 500);
    }

    cacheElements() {
        this.appointmentsContainer = document.getElementById('appointments-container');
        this.emptyState = document.getElementById('empty-state');
        this.modal = document.getElementById('appointment-modal');
        this.form = document.getElementById('appointment-form');
        this.weatherFilter = document.getElementById('weather-filter');
        this.monthFilter = document.getElementById('month-filter');
        this.addBtn = document.getElementById('add-appointment');
        this.closeBtn = document.getElementById('close-modal');
        this.cancelBtn = document.getElementById('cancel-btn');
        
        // Log if any elements are missing
        if (!this.appointmentsContainer) console.error('appointments-container not found');
        if (!this.modal) console.error('appointment-modal not found');
        if (!this.form) console.error('appointment-form not found');
        if (!this.addBtn) console.error('add-appointment button not found');
        
        console.log('Elements cached successfully');
    }

    bindEvents() {
        if (this.addBtn) {
            this.addBtn.addEventListener('click', () => {
                console.log('Add button clicked');
                this.openModal();
            });
        }
        
        if (this.closeBtn) {
            this.closeBtn.addEventListener('click', () => {
                console.log('Close button clicked');
                this.closeModal();
            });
        }
        
        if (this.cancelBtn) {
            this.cancelBtn.addEventListener('click', () => {
                console.log('Cancel button clicked');
                this.closeModal();
            });
        }
        
        if (this.form) {
            this.form.addEventListener('submit', (e) => {
                console.log('Form submitted');
                this.handleFormSubmit(e);
            });
        }
        
        if (this.weatherFilter) {
            this.weatherFilter.addEventListener('change', () => {
                console.log('Weather filter changed');
                this.renderAppointments();
            });
        }
        
        if (this.monthFilter) {
            this.monthFilter.addEventListener('change', () => {
                console.log('Month filter changed');
                this.renderAppointments();
            });
        }
        
        // Close modal on outside click
        if (this.modal) {
            this.modal.addEventListener('click', (e) => {
                if (e.target === this.modal) {
                    console.log('Modal background clicked');
                    this.closeModal();
                }
            });
        }
        
        // Add keyboard navigation support (Escape to close modal)
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && !this.modal.classList.contains('hidden')) {
                console.log('Escape key pressed');
                this.closeModal();
            }
        });
    }

    openModal() {
        console.log('Opening modal...');
        this.modal.classList.remove('hidden');
        this.form.reset();
        // Set default date to today
        const dateInput = document.getElementById('date');
        dateInput.valueAsDate = new Date();
        // Focus on the first input field
        setTimeout(() => {
            document.getElementById('title').focus();
        }, 100);
        console.log('Modal opened, ready for input');
    }

    closeModal() {
        this.modal.classList.add('hidden');
    }

    handleFormSubmit(e) {
        e.preventDefault();
        
        const appointment = {
            id: this.generateId(),
            title: document.getElementById('title').value,
            date: document.getElementById('date').value,
            time: document.getElementById('time').value,
            duration: parseFloat(document.getElementById('duration').value),
            weatherType: document.getElementById('weather-type').value,
            temperature: parseInt(document.getElementById('temperature').value),
            location: document.getElementById('location').value,
            notes: document.getElementById('notes').value,
            createdAt: new Date().toISOString()
        };

        this.appointments.push(appointment);
        this.saveToStorage();
        this.renderAppointments();
        this.updateStats();
        this.closeModal();
        this.showNotification('Appointment added successfully! 🎉');
    }

    generateId() {
        return Date.now().toString(36) + Math.random().toString(36).substr(2);
    }

    getFilteredAppointments() {
        const weatherFilter = this.weatherFilter.value;
        const monthFilter = this.monthFilter.value;

        return this.appointments.filter(appointment => {
            // Filter by weather type
            if (weatherFilter !== 'all' && appointment.weatherType !== weatherFilter) {
                return false;
            }

            // Filter by month
            if (monthFilter !== 'all') {
                const appointmentMonth = new Date(appointment.date).getMonth() + 1;
                if (appointmentMonth !== parseInt(monthFilter)) {
                    return false;
                }
            }

            return true;
        }).sort((a, b) => new Date(a.date) - new Date(b.date));
    }

    renderAppointments() {
        const filtered = this.getFilteredAppointments();

        if (filtered.length === 0) {
            this.appointmentsContainer.innerHTML = '';
            this.emptyState.classList.remove('hidden');
            return;
        }

        this.emptyState.classList.add('hidden');
        
        this.appointmentsContainer.innerHTML = filtered.map(appointment => 
            this.createAppointmentCard(appointment)
        ).join('');

        // Add delete event listeners
        this.appointmentsContainer.querySelectorAll('.btn-delete').forEach(btn => {
            btn.addEventListener('click', (e) => {
                const card = e.target.closest('.appointment-card');
                const id = card.dataset.id;
                this.deleteAppointment(id);
            });
        });
    }

    createAppointmentCard(appointment) {
        const dateObj = new Date(appointment.date);
        const dateString = dateObj.toLocaleDateString('en-US', { 
            weekday: 'short', 
            year: 'numeric', 
            month: 'short', 
            day: 'numeric' 
        });
        
        const weatherIcons = {
            'sunny': '☀️',
            'partly-cloudy': '⛅',
            'clear': '🌟',
            'warm': '🌡️'
        };

        const weatherLabels = {
            'sunny': 'Sunny',
            'partly-cloudy': 'Partly Cloudy',
            'clear': 'Clear Sky',
            'warm': 'Warm Day'
        };

        return `
            <div class="appointment-card ${appointment.weatherType}" data-id="${appointment.id}">
                <div class="appointment-header">
                    <div>
                        <div class="appointment-title">${this.escapeHtml(appointment.title)}</div>
                        <div class="appointment-date">${dateString} at ${appointment.time}</div>
                    </div>
                    <div class="appointment-weather">
                        <span>${weatherIcons[appointment.weatherType]}</span>
                        <span>${weatherLabels[appointment.weatherType]}</span>
                    </div>
                </div>
                
                <div class="appointment-details">
                    <div class="detail-item">
                        <span class="detail-icon">🌡️</span>
                        <span>${appointment.temperature}°F</span>
                    </div>
                    <div class="detail-item">
                        <span class="detail-icon">⏱️</span>
                        <span>${appointment.duration} hours</span>
                    </div>
                </div>

                ${appointment.location ? `
                    <div class="appointment-location">
                        <span class="detail-icon">📍</span>
                        <span>${this.escapeHtml(appointment.location)}</span>
                    </div>
                ` : ''}

                ${appointment.notes ? `
                    <div class="appointment-notes">
                        ${this.escapeHtml(appointment.notes)}
                    </div>
                ` : ''}

                <div class="appointment-actions">
                    <button class="btn-icon btn-delete" title="Delete Appointment">
                        🗑️
                    </button>
                </div>
            </div>
        `;
    }

    deleteAppointment(id) {
        if (!confirm('Are you sure you want to delete this appointment?')) {
            return;
        }

        this.appointments = this.appointments.filter(a => a.id !== id);
        this.saveToStorage();
        this.renderAppointments();
        this.updateStats();
        this.showNotification('Appointment deleted');
    }

    updateStats() {
        const now = new Date();
        const currentMonth = now.getMonth() + 1;
        const currentYear = now.getFullYear();

        const totalCount = this.appointments.length;
        const thisMonthCount = this.appointments.filter(app => {
            const appDate = new Date(app.date);
            return appDate.getMonth() + 1 === currentMonth && 
                   appDate.getFullYear() === currentYear;
        }).length;

        const temps = this.appointments.map(a => a.temperature);
        const avgTemp = temps.length > 0 
            ? (temps.reduce((a, b) => a + b, 0) / temps.length).toFixed(1) 
            : '--';

        // Animate numbers
        this.animateValue('total-count', 0, totalCount, 500);
        this.animateValue('this-month', 0, thisMonthCount, 500);
        
        const avgTempEl = document.getElementById('avg-temp');
        avgTempEl.textContent = avgTemp + '°';
    }

    animateValue(elementId, start, end, duration) {
        const element = document.getElementById(elementId);
        const range = end - start;
        const startTime = performance.now();

        const update = (currentTime) => {
            const elapsed = currentTime - startTime;
            const progress = Math.min(elapsed / duration, 1);
            const easeProgress = 1 - Math.pow(1 - progress, 3); // easeOutCubic
            const current = Math.floor(start + range * easeProgress);
            
            element.textContent = current;

            if (progress < 1) {
                requestAnimationFrame(update);
            }
        };

        requestAnimationFrame(update);
    }

    saveToStorage() {
        localStorage.setItem('weatherAppointments', JSON.stringify(this.appointments));
    }

    loadFromStorage() {
        try {
            const stored = localStorage.getItem('weatherAppointments');
            return stored ? JSON.parse(stored) : [];
        } catch (e) {
            console.error('Error loading appointments:', e);
            return [];
        }
    }

    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    showNotification(message) {
        // Create notification element
        const notification = document.createElement('div');
        notification.style.cssText = `
            position: fixed;
            bottom: 2rem;
            right: 2rem;
            background: #4ECDC4;
            color: white;
            padding: 1rem 2rem;
            border-radius: 12px;
            box-shadow: 0 4px 16px rgba(0,0,0,0.2);
            font-weight: 600;
            animation: slideUp 0.3s ease;
            z-index: 2000;
        `;
        notification.textContent = message;
        document.body.appendChild(notification);

        // Remove after 3 seconds
        setTimeout(() => {
            notification.style.animation = 'fadeIn 0.3s ease reverse';
            setTimeout(() => notification.remove(), 300);
        }, 3000);
    }
}

// Initialize app when DOM is ready
document.addEventListener('DOMContentLoaded', () => {
    console.log('DOM loaded, initializing app...');
    window.weatherApp = new WeatherAppointments();
    console.log('Weather Appointments app initialized!');
    console.log('Try clicking the "+ Add Appointment" button to add a new appointment.');
});
