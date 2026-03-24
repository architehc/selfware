// Form Validation JavaScript

// Validation rules configuration
const validationRules = {
  required: (value) => {
    if (!value || value.trim() === '') {
      return 'This field is required';
    }
    return null;
  },
  email: (value) => {
    const emailRegex = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;
    if (!emailRegex.test(value)) {
      return 'Please enter a valid email address';
    }
    return null;
  },
  minLength: (minLength) => (value) => {
    if (value.length < minLength) {
      return `Must be at least ${minLength} characters`;
    }
    return null;
  },
  maxLength: (maxLength) => (value) => {
    if (value.length > maxLength) {
      return `Must be no more than ${maxLength} characters`;
    }
    return null;
  },
  pattern: (regex, message) => (value) => {
    if (!regex.test(value)) {
      return message || 'Invalid format';
    }
    return null;
  },
  number: (value) => {
    if (value && isNaN(Number(value))) {
      return 'Must be a number';
    }
    return null;
  },
  min: (minValue) => (value) => {
    if (value && Number(value) < minValue) {
      return `Must be at least ${minValue}`;
    }
    return null;
  },
  max: (maxValue) => (value) => {
    if (value && Number(value) > maxValue) {
      return `Must be no more than ${maxValue}`;
    }
    return null;
  },
  matches: (field1, field2) => (value, formData) => {
    if (formData[field1] !== formData[field2]) {
      return 'Fields do not match';
    }
    return null;
  },
  url: (value) => {
    try {
      new URL(value);
      return null;
    } catch {
      return 'Please enter a valid URL';
    }
  },
  phone: (value) => {
    const phoneRegex = /^[\+]?[(]?[0-9]{3}[)]?[-\s\.]?[0-9]{3}[-\s\.]?[0-9]{4,6}$/;
    if (!phoneRegex.test(value.replace(/\s/g, ''))) {
      return 'Please enter a valid phone number';
    }
    return null;
  }
};

// Form validator class
class FormValidator {
  constructor(formElement) {
    this.form = formElement;
    this.errors = {};
    this.formData = {};
  }

  // Add validation rules to a field
  addField(fieldName, rules) {
    if (!this.fields) {
      this.fields = {};
    }
    this.fields[fieldName] = rules;
  }

  // Validate all fields
  validate() {
    this.errors = {};
    this.formData = {};

    // Collect form data
    const formData = new FormData(this.form);
    for (let [key, value] of formData.entries()) {
      this.formData[key] = value;
    }

    // Validate each field
    for (const [fieldName, rules] of Object.entries(this.fields || {})) {
      const value = this.formData[fieldName] || '';
      
      for (const rule of rules) {
        const ruleName = rule.name;
        const ruleArgs = rule.args || [];
        
        let error;
        if (ruleName === 'matches') {
          error = validationRules[ruleName](...ruleArgs)(value, this.formData);
        } else {
          error = validationRules[ruleName](...ruleArgs)(value);
        }

        if (error) {
          this.errors[fieldName] = error;
          this.showFieldError(fieldName, error);
          break; // Stop at first error for this field
        } else {
          this.clearFieldError(fieldName);
        }
      }
    }

    return Object.keys(this.errors).length === 0;
  }

  // Show error message for a field
  showFieldError(fieldName, message) {
    const field = this.form.querySelector(`[name="${fieldName}"]`);
    if (!field) return;

    // Remove existing error message
    const existingError = field.parentElement.querySelector('.error-message');
    if (existingError) {
      existingError.remove();
    }

    // Add error styling
    field.classList.add('error');

    // Create error message element
    const errorElement = document.createElement('div');
    errorElement.className = 'error-message';
    errorElement.textContent = message;
    
    field.parentElement.appendChild(errorElement);
  }

  // Clear error message for a field
  clearFieldError(fieldName) {
    const field = this.form.querySelector(`[name="${fieldName}"]`);
    if (!field) return;

    field.classList.remove('error');

    const errorElement = field.parentElement.querySelector('.error-message');
    if (errorElement) {
      errorElement.remove();
    }
  }

  // Clear all errors
  clearAllErrors() {
    const errorFields = this.form.querySelectorAll('.error');
    errorFields.forEach(field => {
      this.clearFieldError(field.name);
    });
  }

  // Get form data (only if valid)
  getFormData() {
    return this.validate() ? this.formData : null;
  }

  // Submit handler
  onSubmit(handler) {
    this.form.addEventListener('submit', (e) => {
      e.preventDefault();
      
      if (this.validate()) {
        const formData = this.getFormData();
        handler(formData);
      }
    });
  }
}

// Convenience function to create validator with rules
function createValidator(formSelector, rules) {
  const form = document.querySelector(formSelector);
  if (!form) {
    console.error(`Form ${formSelector} not found`);
    return null;
  }

  const validator = new FormValidator(form);
  
  // Convert rules object to field definitions
  for (const [fieldName, fieldRules] of Object.entries(rules)) {
    const parsedRules = fieldRules.map(rule => {
      if (typeof rule === 'string') {
        return { name: rule };
      } else if (Array.isArray(rule)) {
        return { name: rule[0], args: rule.slice(1) };
      } else if (typeof rule === 'object') {
        return rule;
      }
    });
    validator.addField(fieldName, parsedRules);
  }

  return validator;
}

// Example usage:
// const validator = createValidator('#contactForm', {
//   name: ['required', ['minLength', 2]],
//   email: ['required', 'email'],
//   password: ['required', ['minLength', 8], ['maxLength', 20]],
//   confirmPassword: ['required', ['matches', 'password', 'confirmPassword']],
//   phone: ['phone'],
//   website: ['url']
// });
//
// validator.onSubmit((formData) => {
//   console.log('Form submitted:', formData);
//   // Handle form submission (e.g., send to server)
// });

// Export for module usage
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { FormValidator, createValidator, validationRules };
}
