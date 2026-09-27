public abstract class Animal {
    protected String nombre;
    protected int edad;

    public Animal(String nombre, int edad) {
        this.nombre = nombre;
        this.edad = edad;
    }

    public abstract String hablar();

    public String descripcion() {
        return nombre + " (" + edad + " años) dice " + hablar();
    }

    public String getNombre() {
        return nombre;
    }
}